import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.mojang.brigadier.tree.ArgumentCommandNode;
import com.mojang.brigadier.tree.CommandNode;
import com.mojang.brigadier.tree.LiteralCommandNode;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.Field;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.Callable;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.resources.RegistryOps;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.player.Abilities;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.scores.Objective;
import net.minecraft.world.scores.PlayerTeam;
import net.minecraft.world.scores.Scoreboard;

/**
 * Builds the state snapshot (format 2, see tests/e2e/client/PROTOCOL.md). Runs on the render thread. Every section is
 * read on its own: one that throws becomes {"error": text} and the others are unaffected. Members the client does not
 * expose are read by reflection.
 */
final class E2eSnapshot {
    /** What the `probe` control command asked the next snapshots to read from the level (null: no `probe` section). */
    static JsonObject probeSpec;

    private final Minecraft mc;
    private RegistryOps<JsonElement> ops;

    private E2eSnapshot(Minecraft mc) {
        this.mc = mc;
    }

    static JsonObject build(Minecraft mc) {
        return new E2eSnapshot(mc).snapshot();
    }

    /** The translation key of a widget's message (`gui.proceed`), null for literal text. */
    static String messageKey(AbstractWidget w) {
        return w.getMessage().getContents() instanceof net.minecraft.network.chat.contents.TranslatableContents t ? t.getKey() : null;
    }

    /** The clickable-or-not widgets of a screen, in the order the snapshot lists them. */
    static List<AbstractWidget> widgets(Screen screen) {
        List<AbstractWidget> out = new ArrayList<>();
        for (GuiEventListener l : screen.children()) {
            if (l instanceof AbstractWidget w) out.add(w);
        }
        return out;
    }

    private static JsonElement section(Callable<JsonElement> body) {
        try {
            JsonElement e = body.call();
            return e == null ? JsonNull.INSTANCE : e;
        } catch (Throwable t) {
            JsonObject o = new JsonObject();
            o.addProperty("error", t.toString());
            return o;
        }
    }

    private JsonObject snapshot() {
        ClientPacketListener conn = mc.getConnection();
        ClientLevel level = mc.level;
        Player player = mc.player;
        boolean inLevel = conn != null && level != null && player != null;
        if (inLevel) ops = conn.registryAccess().createSerializationContext(JsonOps.INSTANCE);
        JsonObject o = new JsonObject();
        o.addProperty("format", 2);
        o.add("screen", section(this::screen));
        o.add("menu", section(() -> inLevel ? menu(player.containerMenu) : null));
        o.add("player", section(() -> inLevel ? player() : null));
        o.add("inventory", section(() -> inLevel ? inventory() : null));
        o.add("level", section(() -> inLevel ? level() : null));
        o.add("entities", section(() -> inLevel ? entities() : null));
        o.add("registries", section(() -> inLevel ? registries(conn.registryAccess(), false) : null));
        o.add("tags", section(() -> inLevel ? registries(conn.registryAccess(), true) : null));
        o.add("features", section(() -> inLevel ? features() : null));
        o.add("recipes", section(() -> inLevel ? recipes() : null));
        o.add("advancements", section(() -> inLevel ? advancements() : null));
        o.add("commands", section(() -> inLevel ? commands(conn.getCommands().getRoot()) : null));
        o.add("chat", section(this::chat));
        o.add("scoreboard", section(() -> inLevel ? scoreboard() : null));
        o.add("tab_list", section(() -> inLevel ? tabList() : null));
        o.add("boss_bars", section(this::bossBars));
        o.add("titles", section(this::titles));
        o.add("packs", section(this::packs));
        o.add("server", section(() -> inLevel ? server() : null));
        o.add("waypoints", section(() -> inLevel ? waypoints(conn) : null));
        o.add("probe", section(() -> inLevel && probeSpec != null ? probe() : null));
        o.add("packets", section(() -> {
            JsonObject p = new JsonObject();
            p.add("in", E2eProbe.counts(E2eProbe.IN));
            p.add("out", E2eProbe.counts(E2eProbe.OUT));
            return p;
        }));
        return o;
    }

    // ---- helpers ----

    private static JsonArray arr(double... v) {
        JsonArray a = new JsonArray();
        for (double d : v) a.add(d);
        return a;
    }

    private static Object field(Object target, String name) throws ReflectiveOperationException {
        for (Class<?> c = target.getClass(); c != null; c = c.getSuperclass()) {
            try {
                Field f = c.getDeclaredField(name);
                f.setAccessible(true);
                return f.get(target);
            } catch (NoSuchFieldException ignored) {
                // look in the superclass
            }
        }
        throw new NoSuchFieldException(target.getClass().getName() + "." + name);
    }

    private JsonElement item(ItemStack stack) {
        if (stack.isEmpty()) return new JsonObject();
        return ItemStack.OPTIONAL_CODEC.encodeStart(ops, stack).getOrThrow();
    }

    private static String text(Component c) {
        return c == null ? null : c.getString();
    }

    /** A synched entity data value as JSON: scalars as they are, components as text, stacks as their codec JSON. */
    private JsonElement value(Object v) {
        if (v == null) return JsonNull.INSTANCE;
        if (v instanceof Number n) return new com.google.gson.JsonPrimitive(n);
        if (v instanceof Boolean b) return new com.google.gson.JsonPrimitive(b);
        if (v instanceof ItemStack s) return item(s);
        if (v instanceof Component c) return new com.google.gson.JsonPrimitive(c.getString());
        if (v instanceof Optional<?> opt) return opt.isPresent() ? value(opt.get()) : JsonNull.INSTANCE;
        if (v instanceof Holder<?> h) return new com.google.gson.JsonPrimitive(String.valueOf(h.getRegisteredName()));
        if (v instanceof List<?> list) {
            JsonArray a = new JsonArray();
            for (Object o : list) a.add(value(o));
            return a;
        }
        // particle options print as an identity hash: read them through their codec
        if (v instanceof net.minecraft.core.particles.ParticleOptions p) {
            return net.minecraft.core.particles.ParticleTypes.CODEC.encodeStart(ops, p).result().orElseGet(() -> new com.google.gson.JsonPrimitive(String.valueOf(p)));
        }
        return new com.google.gson.JsonPrimitive(String.valueOf(v));
    }

    // ---- sections ----

    private JsonElement screen() throws Exception {
        Screen s = mc.gui.screen();
        if (s == null) return null;
        JsonObject o = new JsonObject();
        o.addProperty("class", s.getClass().getSimpleName());
        o.addProperty("title", text(s.getTitle()));
        JsonArray ws = new JsonArray();
        for (AbstractWidget w : widgets(s)) {
            JsonObject j = new JsonObject();
            j.addProperty("class", w.getClass().getSimpleName());
            j.addProperty("message", text(w.getMessage()));
            j.addProperty("key", messageKey(w));
            j.addProperty("x", w.getX());
            j.addProperty("y", w.getY());
            j.addProperty("width", w.getWidth());
            j.addProperty("height", w.getHeight());
            j.addProperty("active", w.active);
            j.addProperty("visible", w.visible);
            ws.add(j);
        }
        o.add("widgets", ws);
        if (s instanceof JoinMultiplayerScreen jm) o.add("servers", servers(jm));
        return o;
    }

    /** The server list of the multiplayer screen with what the status pings filled in. */
    private static JsonElement servers(JoinMultiplayerScreen screen) throws Exception {
        JsonArray out = new JsonArray();
        for (int i = 0; i < screen.getServers().size(); i++) {
            ServerData d = screen.getServers().get(i);
            JsonObject j = new JsonObject();
            j.addProperty("name", d.name);
            j.addProperty("ip", d.ip);
            j.addProperty("state", d.state().name());
            j.addProperty("status", text(d.status));
            j.addProperty("motd", text(d.motd));
            j.addProperty("version", text(d.version));
            j.addProperty("protocol", d.protocol);
            j.addProperty("ping", d.ping);
            if (d.players != null) {
                JsonObject p = new JsonObject();
                p.addProperty("online", d.players.online());
                p.addProperty("max", d.players.max());
                j.add("players", p);
            }
            JsonArray sample = new JsonArray();
            d.playerList.forEach(c -> sample.add(text(c)));
            j.add("player_list", sample);
            byte[] icon = d.getIconBytes();
            j.addProperty("icon_sha1", icon == null ? null : HexFormat.of().formatHex(MessageDigest.getInstance("SHA-1").digest(icon)));
            out.add(j);
        }
        return out;
    }

    private JsonElement menu(AbstractContainerMenu m) {
        JsonObject o = new JsonObject();
        o.addProperty("class", m.getClass().getSimpleName());
        o.addProperty("id", m.containerId);
        o.addProperty("state_id", m.getStateId());
        o.add("carried", item(m.getCarried()));
        JsonArray slots = new JsonArray();
        for (Slot s : m.slots) {
            JsonObject j = new JsonObject();
            j.addProperty("index", s.index);
            j.add("item", item(s.getItem()));
            slots.add(j);
        }
        o.add("slots", slots);
        return o;
    }

    private JsonElement player() {
        var p = mc.player;
        JsonObject o = new JsonObject();
        o.addProperty("name", p.getGameProfile().name());
        o.addProperty("uuid", p.getUUID().toString());
        o.add("pos", arr(p.getX(), p.getY(), p.getZ()));
        o.add("rot", arr(p.getYRot(), p.getXRot()));
        o.add("motion", arr(p.getDeltaMovement().x, p.getDeltaMovement().y, p.getDeltaMovement().z));
        o.addProperty("health", p.getHealth());
        o.addProperty("food", p.getFoodData().getFoodLevel());
        o.addProperty("saturation", p.getFoodData().getSaturationLevel());
        o.addProperty("xp_level", p.experienceLevel);
        o.addProperty("xp_progress", p.experienceProgress);
        o.addProperty("xp_total", p.totalExperience);
        o.addProperty("gamemode", mc.gameMode.getPlayerMode().getName());
        Abilities a = p.getAbilities();
        JsonObject ab = new JsonObject();
        ab.addProperty("invulnerable", a.invulnerable);
        ab.addProperty("flying", a.flying);
        ab.addProperty("may_fly", a.mayfly);
        ab.addProperty("instabuild", a.instabuild);
        ab.addProperty("may_build", a.mayBuild);
        ab.addProperty("fly_speed", a.getFlyingSpeed());
        ab.addProperty("walk_speed", a.getWalkingSpeed());
        o.add("abilities", ab);
        o.add("effects", effects(p));
        o.add("attributes", attributes(p));
        o.addProperty("selected_slot", p.getInventory().getSelectedSlot());
        return o;
    }

    private JsonElement effects(LivingEntity e) {
        JsonArray a = new JsonArray();
        // the client keeps effects in a hash map keyed by identity hash: its order changes from run to run
        List<MobEffectInstance> sorted = new ArrayList<>(e.getActiveEffects());
        sorted.sort(java.util.Comparator.comparing(i -> i.getEffect().getRegisteredName()));
        for (MobEffectInstance i : sorted) {
            JsonObject j = new JsonObject();
            j.addProperty("effect", i.getEffect().getRegisteredName());
            j.addProperty("amplifier", i.getAmplifier());
            j.addProperty("duration", i.getDuration());
            j.addProperty("ambient", i.isAmbient());
            j.addProperty("visible", i.isVisible());
            a.add(j);
        }
        return a;
    }

    private JsonElement attributes(LivingEntity e) {
        JsonObject o = new JsonObject();
        for (AttributeInstance i : e.getAttributes().getSyncableAttributes()) {
            JsonObject j = new JsonObject();
            j.addProperty("base", i.getBaseValue());
            j.addProperty("value", i.getValue());
            o.add(i.getAttribute().getRegisteredName(), j);
        }
        return o;
    }

    private JsonElement inventory() {
        var inv = mc.player.getInventory();
        JsonArray a = new JsonArray();
        for (int i = 0; i < inv.getContainerSize(); i++) {
            JsonObject j = new JsonObject();
            j.addProperty("slot", i);
            j.add("item", item(inv.getItem(i)));
            a.add(j);
        }
        return a;
    }

    /** The locator bar's tracked waypoints sorted by id: kind, icon style and colour, and the position fields of the kind. */
    private JsonElement waypoints(ClientPacketListener conn) throws ReflectiveOperationException {
        List<JsonObject> found = new ArrayList<>();
        List<net.minecraft.world.waypoints.TrackedWaypoint> all = new ArrayList<>();
        Map<?, ?> map = (Map<?, ?>) field(conn.getWaypointManager(), "waypoints");
        for (Object w : map.values()) all.add((net.minecraft.world.waypoints.TrackedWaypoint) w);
        for (var w : all) {
            JsonObject j = new JsonObject();
            j.addProperty("id", w.id().map(java.util.UUID::toString, n -> n));
            j.addProperty("kind", w.getClass().getSimpleName());
            j.addProperty("style", w.icon().style.identifier().toString());
            j.addProperty("color", w.icon().color.map(c -> String.format("%06x", c)).orElse(null));
            for (Field f : w.getClass().getDeclaredFields()) {
                if (java.lang.reflect.Modifier.isStatic(f.getModifiers())) continue;
                f.setAccessible(true);
                j.addProperty(f.getName(), String.valueOf(f.get(w)));
            }
            found.add(j);
        }
        found.sort(java.util.Comparator.comparing(j -> j.get("id").getAsString()));
        JsonArray a = new JsonArray();
        found.forEach(a::add);
        return a;
    }

    private static BlockPos at(JsonArray a, int i) {
        return new BlockPos(a.get(i).getAsInt(), a.get(i + 1).getAsInt(), a.get(i + 2).getAsInt());
    }

    /**
     * The `probe` section: `blocks` boxes `[x1,y1,z1,x2,y2,z2]` as run-length lists of block states (x fastest, then y, then z)
     * with the type and update tag of a block entity, `light` positions as `block/sky` levels, `biomes` positions as biome ids.
     */
    private JsonElement probe() {
        ClientLevel l = mc.level;
        JsonObject o = new JsonObject();
        if (probeSpec.has("blocks")) {
            JsonObject boxes = new JsonObject();
            for (JsonElement e : probeSpec.getAsJsonArray("blocks")) {
                JsonArray b = e.getAsJsonArray();
                BlockPos lo = at(b, 0), hi = at(b, 3);
                JsonArray runs = new JsonArray();
                String last = null;
                int count = 0;
                for (BlockPos p : BlockPos.betweenClosed(lo, hi)) {
                    BlockState st = l.getBlockState(p);
                    String text = st.toString();
                    BlockEntity be = l.getBlockEntity(p);
                    if (be != null) {
                        text += " @" + BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType()) + " " + be.getUpdateTag(l.registryAccess());
                    }
                    if (text.equals(last)) {
                        count++;
                        continue;
                    }
                    if (last != null) runs.add(count + "x " + last);
                    last = text;
                    count = 1;
                }
                if (last != null) runs.add(count + "x " + last);
                boxes.add(lo.getX() + " " + lo.getY() + " " + lo.getZ() + " " + hi.getX() + " " + hi.getY() + " " + hi.getZ(), runs);
            }
            o.add("blocks", boxes);
        }
        if (probeSpec.has("light")) {
            JsonObject out = new JsonObject();
            for (JsonElement e : probeSpec.getAsJsonArray("light")) {
                BlockPos p = at(e.getAsJsonArray(), 0);
                out.addProperty(p.getX() + " " + p.getY() + " " + p.getZ(), l.getBrightness(LightLayer.BLOCK, p) + "/" + l.getBrightness(LightLayer.SKY, p));
            }
            o.add("light", out);
        }
        if (probeSpec.has("biomes")) {
            JsonObject out = new JsonObject();
            for (JsonElement e : probeSpec.getAsJsonArray("biomes")) {
                BlockPos p = at(e.getAsJsonArray(), 0);
                out.addProperty(p.getX() + " " + p.getY() + " " + p.getZ(), l.getBiome(p).getRegisteredName());
            }
            o.add("biomes", out);
        }
        return o;
    }

    private JsonElement level() {
        ClientLevel l = mc.level;
        JsonObject o = new JsonObject();
        o.addProperty("dimension", l.dimension().identifier().toString());
        o.addProperty("game_time", l.getGameTime());
        o.addProperty("day_time", l.getDefaultClockTime());
        o.addProperty("rain", l.getRainLevel(1.0F));
        o.addProperty("thunder", l.getThunderLevel(1.0F));
        o.addProperty("difficulty", l.getDifficulty().getSerializedName());
        var spawn = l.getRespawnData();
        o.addProperty("spawn", spawn.pos().getX() + " " + spawn.pos().getY() + " " + spawn.pos().getZ());
        var b = l.getWorldBorder();
        JsonObject border = new JsonObject();
        border.addProperty("size", b.getSize());
        border.addProperty("center_x", b.getCenterX());
        border.addProperty("center_z", b.getCenterZ());
        border.addProperty("warning_blocks", b.getWarningBlocks());
        border.addProperty("warning_time", b.getWarningTime());
        o.add("border", border);
        o.addProperty("tick_frozen", l.tickRateManager().isFrozen());
        o.addProperty("tick_rate", l.tickRateManager().tickrate());
        o.addProperty("chunks_loaded", l.getChunkSource().getLoadedChunksCount());
        return o;
    }

    private JsonElement entities() {
        JsonArray a = new JsonArray();
        for (Entity e : mc.level.entitiesForRendering()) {
            JsonObject o = new JsonObject();
            o.addProperty("id", e.getId());
            o.addProperty("uuid", e.getUUID().toString());
            o.addProperty("type", BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString());
            o.add("pos", arr(e.getX(), e.getY(), e.getZ()));
            o.add("rot", arr(e.getYRot(), e.getXRot()));
            o.add("motion", arr(e.getDeltaMovement().x, e.getDeltaMovement().y, e.getDeltaMovement().z));
            JsonObject data = new JsonObject();
            List<SynchedEntityData.DataValue<?>> nonDefault = e.getEntityData().getNonDefaultValues();
            // null when every value is still its default
            for (SynchedEntityData.DataValue<?> v : nonDefault == null ? List.<SynchedEntityData.DataValue<?>>of() : nonDefault) {
                data.add(Integer.toString(v.id()), value(v.value()));
            }
            o.add("data", data);
            JsonArray pass = new JsonArray();
            for (Entity p : e.getPassengers()) pass.add(p.getId());
            o.add("passengers", pass);
            if (e instanceof LivingEntity le) {
                JsonObject eq = new JsonObject();
                for (EquipmentSlot s : EquipmentSlot.VALUES) {
                    ItemStack st = le.getItemBySlot(s);
                    if (!st.isEmpty()) eq.add(s.getName(), item(st));
                }
                o.add("equipment", eq);
                o.add("attributes", attributes(le));
                o.add("effects", effects(le));
            }
            a.add(o);
        }
        return a;
    }

    private JsonElement registries(RegistryAccess access, boolean tags) {
        JsonObject o = new JsonObject();
        access.registries().forEach(r -> {
            var reg = r.value();
            o.addProperty(r.key().identifier().toString(), tags ? reg.getTags().count() : reg.size());
        });
        return o;
    }

    private JsonElement features() {
        JsonArray a = new JsonArray();
        FeatureFlags.REGISTRY.toNames(mc.getConnection().enabledFeatures()).stream().map(Object::toString).sorted().forEach(a::add);
        return a;
    }

    private JsonElement recipes() {
        var book = mc.player.getRecipeBook();
        JsonObject o = new JsonObject();
        o.addProperty("collections", book.getCollections().size());
        o.addProperty("recipes", book.getCollections().stream().mapToInt(c -> c.getRecipes().size()).sum());
        return o;
    }

    private JsonElement advancements() {
        var adv = mc.getConnection().getAdvancements();
        JsonObject o = new JsonObject();
        o.addProperty("nodes", adv.tree().nodes().size());
        int roots = 0;
        for (var r : adv.tree().roots()) roots++;
        o.addProperty("roots", roots);
        int done = 0;
        for (var p : adv.progress().values()) if (p.isDone()) done++;
        o.addProperty("progress_entries", adv.progress().size());
        o.addProperty("done", done);
        return o;
    }

    private JsonElement commands(CommandNode<?> node) {
        JsonObject o = new JsonObject();
        o.addProperty("name", node.getName());
        String kind = node instanceof LiteralCommandNode ? "literal" : node instanceof ArgumentCommandNode ? "argument" : "root";
        o.addProperty("kind", kind);
        if (node instanceof ArgumentCommandNode<?, ?> arg) o.addProperty("parser", arg.getType().getClass().getSimpleName());
        o.addProperty("executable", node.getCommand() != null);
        if (node.getRedirect() != null) o.addProperty("redirect", node.getRedirect().getName());
        Collection<? extends CommandNode<?>> kids = node.getChildren();
        if (!kids.isEmpty()) {
            JsonArray a = new JsonArray();
            for (CommandNode<?> k : kids) a.add(commands(k));
            o.add("children", a);
        }
        return o;
    }

    private JsonElement chat() throws ReflectiveOperationException {
        Object chat = field(field(mc.gui, "hud"), "chat");
        JsonArray a = new JsonArray();
        for (Object m : (List<?>) field(chat, "allMessages")) {
            Component c = (Component) field(m, "content");
            a.add(c.getString());
        }
        return a;
    }

    private JsonElement scoreboard() {
        Scoreboard sb = mc.level.getScoreboard();
        JsonObject o = new JsonObject();
        JsonArray objs = new JsonArray();
        for (Objective ob : sb.getObjectives()) {
            JsonObject j = new JsonObject();
            j.addProperty("name", ob.getName());
            j.addProperty("display", text(ob.getDisplayName()));
            j.addProperty("criteria", ob.getCriteria().getName());
            JsonObject scores = new JsonObject();
            sb.listPlayerScores(ob).forEach(s -> scores.addProperty(s.owner(), s.value()));
            j.add("scores", scores);
            objs.add(j);
        }
        o.add("objectives", objs);
        JsonArray teams = new JsonArray();
        for (PlayerTeam t : sb.getPlayerTeams()) {
            JsonObject j = new JsonObject();
            j.addProperty("name", t.getName());
            j.addProperty("display", text(t.getDisplayName()));
            JsonArray members = new JsonArray();
            t.getPlayers().stream().sorted().forEach(members::add);
            j.add("members", members);
            teams.add(j);
        }
        o.add("teams", teams);
        return o;
    }

    private JsonElement tabList() throws ReflectiveOperationException {
        JsonObject o = new JsonObject();
        JsonArray a = new JsonArray();
        for (PlayerInfo i : mc.getConnection().getOnlinePlayers()) {
            JsonObject j = new JsonObject();
            j.addProperty("name", i.getProfile().name());
            j.addProperty("uuid", i.getProfile().id().toString());
            j.addProperty("gamemode", i.getGameMode().getName());
            j.addProperty("latency", i.getLatency());
            j.addProperty("display", text(i.getTabListDisplayName()));
            a.add(j);
        }
        o.add("players", a);
        Object overlay = field(field(mc.gui, "hud"), "tabList");
        o.addProperty("header", text((Component) field(overlay, "header")));
        o.addProperty("footer", text((Component) field(overlay, "footer")));
        return o;
    }

    private JsonElement bossBars() throws ReflectiveOperationException {
        Object overlay = field(field(mc.gui, "hud"), "bossOverlay");
        JsonArray a = new JsonArray();
        for (Object e : ((Map<?, ?>) field(overlay, "events")).values()) {
            var b = (net.minecraft.client.gui.components.LerpingBossEvent) e;
            JsonObject j = new JsonObject();
            j.addProperty("uuid", b.getId().toString());
            j.addProperty("name", text(b.getName()));
            j.addProperty("progress", b.getProgress());
            j.addProperty("color", b.getColor().getName());
            j.addProperty("overlay", b.getOverlay().getName());
            a.add(j);
        }
        return a;
    }

    private JsonElement titles() throws ReflectiveOperationException {
        Object hud = field(mc.gui, "hud");
        JsonObject o = new JsonObject();
        o.addProperty("title", text((Component) field(hud, "title")));
        o.addProperty("subtitle", text((Component) field(hud, "subtitle")));
        return o;
    }

    private JsonElement packs() {
        JsonArray a = new JsonArray();
        mc.getResourcePackRepository().getSelectedIds().forEach(a::add);
        return a;
    }

    private JsonElement server() {
        var conn = mc.getConnection();
        JsonObject o = new JsonObject();
        o.addProperty("brand", conn.serverBrand());
        var sd = mc.getCurrentServer();
        o.addProperty("address", sd == null ? null : sd.ip);
        JsonArray links = new JsonArray();
        conn.serverLinks().entries().forEach(e -> links.add(text(e.displayName()) + " " + e.link()));
        o.add("links", links);
        return o;
    }
}
