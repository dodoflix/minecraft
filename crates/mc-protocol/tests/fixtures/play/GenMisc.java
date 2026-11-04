// Golden generator for the `misc` play packets (stats, boss events, combat, scoreboard, advancements, waypoints, tag queries).
// Run: crates/mc-protocol/tests/fixtures/play/gen.sh GenMisc.java   (writes misc.golden)
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.objects.Object2IntLinkedOpenHashMap;
import java.lang.reflect.Constructor;
import java.util.*;
import net.minecraft.advancements.*;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.ClientAsset;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.*;
import net.minecraft.network.chat.numbers.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Vec3i;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.stats.Stat;
import net.minecraft.stats.Stats;
import net.minecraft.world.BossEvent;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import net.minecraft.world.waypoints.TrackedWaypoint;
import net.minecraft.world.waypoints.Waypoint;
import net.minecraft.world.waypoints.WaypointStyleAssets;

public class GenMisc {
   static final UUID U1 = new UUID(0x0123456789abcdefL, 0xfedcba9876543210L);
   static final UUID U2 = new UUID(-1L, Long.MIN_VALUE);
   static final UUID U3 = new UUID(0L, 0L);
   static final String NON_BMP = "\uD83D\uDE00 smile \u00e9";

   static BossEvent boss(UUID id, Component name, BossEvent.BossBarColor c, BossEvent.BossBarOverlay o, float progress, boolean d, boolean m, boolean f) {
      BossEvent e = new BossEvent(id, name, c, o) {};
      e.setProgress(progress);
      e.setDarkenScreen(d).setPlayBossMusic(m).setCreateWorldFog(f);
      return e;
   }

   static Identifier id(String s) { return Identifier.parse(s); }

   @SuppressWarnings("unchecked")
   static ClientboundSetPlayerTeamPacket team(String name, int method, ClientboundSetPlayerTeamPacket.Parameters p, Collection<String> players) throws Exception {
      Constructor<ClientboundSetPlayerTeamPacket> c = ClientboundSetPlayerTeamPacket.class.getDeclaredConstructor(String.class, int.class, Optional.class, Collection.class);
      c.setAccessible(true);
      return c.newInstance(name, method, Optional.ofNullable(p), players);
   }

   static AdvancementProgress progress(Object... kv) throws Exception {
      Map<String, CriterionProgress> m = new LinkedHashMap<>();
      for (int i = 0; i < kv.length; i += 2) m.put((String) kv[i], new CriterionProgress((java.time.Instant) kv[i + 1]));
      Constructor<AdvancementProgress> c = AdvancementProgress.class.getDeclaredConstructor(Map.class);
      c.setAccessible(true);
      return c.newInstance(m);
   }

   static TrackedWaypoint named(String name, Waypoint.Icon icon, int type, java.util.function.Consumer<FriendlyByteBuf> payload) {
      FriendlyByteBuf b = new FriendlyByteBuf(Unpooled.buffer());
      b.writeEither(com.mojang.datafixers.util.Either.<UUID, String>right(name), (buf, u) -> {}, FriendlyByteBuf::writeUtf);
      Waypoint.Icon.STREAM_CODEC.encode(b, icon);
      b.writeVarInt(type);
      payload.accept(b);
      return TrackedWaypoint.STREAM_CODEC.decode(b);
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static ClientboundTrackedWaypointPacket wpPacket(String op, TrackedWaypoint w) throws Exception {
      Class<?> opClass = Class.forName("net.minecraft.network.protocol.game.ClientboundTrackedWaypointPacket$Operation");
      Constructor<?> c = ClientboundTrackedWaypointPacket.class.getDeclaredConstructors()[0];
      c.setAccessible(true);
      return (ClientboundTrackedWaypointPacket) c.newInstance(Enum.valueOf((Class) opClass, op), w);
   }

   static Waypoint.Icon icon(String style, Integer color) {
      Waypoint.Icon i = new Waypoint.Icon();
      i.style = ResourceKey.create(WaypointStyleAssets.ROOT_ID, id(style));
      i.color = Optional.ofNullable(color);
      return i;
   }

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);
      var cb = (java.util.function.BiConsumer<String, net.minecraft.network.protocol.Packet<?>>) PlayHarness::cb;
      var sb = (java.util.function.BiConsumer<String, net.minecraft.network.protocol.Packet<?>>) PlayHarness::sb;

      // ---- award_stats
      cb.accept("award_stats/empty", new ClientboundAwardStatsPacket(new Object2IntLinkedOpenHashMap<Stat<?>>()));
      var stats = new Object2IntLinkedOpenHashMap<Stat<?>>();
      stats.put(Stats.BLOCK_MINED.get(Blocks.STONE), 5);
      stats.put(Stats.ITEM_CRAFTED.get(Items.DIAMOND_PICKAXE), 0);
      stats.put(Stats.ITEM_USED.get(Items.ENDER_PEARL), 300);
      stats.put(Stats.ITEM_BROKEN.get(Items.IRON_SWORD), 1);
      stats.put(Stats.ITEM_PICKED_UP.get(Items.DIRT), 70000);
      stats.put(Stats.ITEM_DROPPED.get(Items.COBBLESTONE), 16384);
      stats.put(Stats.ENTITY_KILLED.get(EntityTypes.ZOMBIE), 12);
      stats.put(Stats.ENTITY_KILLED_BY.get(EntityTypes.CREEPER), 2);
      stats.put(Stats.CUSTOM.get(Stats.JUMP), 1234567);
      stats.put(Stats.CUSTOM.get(Stats.PLAY_TIME), -1);
      stats.put(Stats.BLOCK_MINED.get(Blocks.AIR), Integer.MAX_VALUE);
      stats.put(Stats.BLOCK_MINED.get(Blocks.REINFORCED_DEEPSLATE), Integer.MIN_VALUE);
      cb.accept("award_stats/all", new ClientboundAwardStatsPacket(stats));
      for (var e : new Object[][] {{"block_mined", Stats.BLOCK_MINED.get(Blocks.DIAMOND_ORE)}, {"entity_killed", Stats.ENTITY_KILLED.get(EntityTypes.PLAYER)}, {"custom_first", Stats.CUSTOM.get(Stats.LEAVE_GAME)}}) {
         var one = new Object2IntLinkedOpenHashMap<Stat<?>>();
         one.put((Stat<?>) e[1], 42);
         cb.accept("award_stats/one_" + e[0], new ClientboundAwardStatsPacket(one));
      }

      // ---- boss_event
      var plain = Component.literal("Ender Dragon");
      var styled = Component.translatable("entity.minecraft.wither").withStyle(s -> s.withColor(0xff5555).withBold(true)).append(Component.literal(NON_BMP));
      int n = 0;
      for (var color : BossEvent.BossBarColor.values()) {
         for (var overlay : BossEvent.BossBarOverlay.values()) {
            var ev = boss(U1, n % 2 == 0 ? plain : styled, color, overlay, 0.25f * (n % 5), n % 2 == 0, n % 3 == 0, n % 5 == 0);
            cb.accept("boss_event/add_" + color.name() + "_" + overlay.name(), ClientboundBossEventPacket.createAddPacket(ev));
            n++;
         }
      }
      var ev = boss(U2, plain, BossEvent.BossBarColor.PINK, BossEvent.BossBarOverlay.PROGRESS, 0.5f, false, false, false);
      cb.accept("boss_event/remove", ClientboundBossEventPacket.createRemovePacket(U2));
      cb.accept("boss_event/remove_zero_uuid", ClientboundBossEventPacket.createRemovePacket(U3));
      for (float p : new float[] {0f, 1f, 0.333f, -1.5f, Float.NaN, Float.POSITIVE_INFINITY, 1e-45f}) {
         ev.setProgress(p);
         cb.accept("boss_event/progress_" + Float.floatToRawIntBits(p), ClientboundBossEventPacket.createUpdateProgressPacket(ev));
      }
      ev.setName(styled);
      cb.accept("boss_event/name_styled", ClientboundBossEventPacket.createUpdateNamePacket(ev));
      ev.setName(Component.empty());
      cb.accept("boss_event/name_empty", ClientboundBossEventPacket.createUpdateNamePacket(ev));
      for (var color : BossEvent.BossBarColor.values()) {
         ev.setColor(color);
         ev.setOverlay(BossEvent.BossBarOverlay.values()[color.ordinal() % 5]);
         cb.accept("boss_event/style_" + color.name(), ClientboundBossEventPacket.createUpdateStylePacket(ev));
      }
      for (int bits = 0; bits < 8; bits++) {
         ev.setDarkenScreen((bits & 1) != 0).setPlayBossMusic((bits & 2) != 0).setCreateWorldFog((bits & 4) != 0);
         cb.accept("boss_event/properties_" + bits, ClientboundBossEventPacket.createUpdatePropertiesPacket(ev));
      }

      // ---- combat
      for (int d : new int[] {0, 1, 127, 128, 20000, Integer.MAX_VALUE, -1}) cb.accept("player_combat_end/" + d, new ClientboundPlayerCombatEndPacket(d));
      cb.accept("player_combat_enter", ClientboundPlayerCombatEnterPacket.INSTANCE);
      cb.accept("player_combat_kill/plain", new ClientboundPlayerCombatKillPacket(7, Component.translatable("death.attack.generic", "Steve")));
      cb.accept("player_combat_kill/big_id_styled", new ClientboundPlayerCombatKillPacket(Integer.MAX_VALUE, styled));
      cb.accept("player_combat_kill/negative_empty", new ClientboundPlayerCombatKillPacket(-1, Component.empty()));

      // ---- reset_score
      cb.accept("reset_score/owner_only", new ClientboundResetScorePacket("Steve", null));
      cb.accept("reset_score/with_objective", new ClientboundResetScorePacket("Steve", "kills"));
      cb.accept("reset_score/unicode", new ClientboundResetScorePacket(NON_BMP, NON_BMP));
      cb.accept("reset_score/empty", new ClientboundResetScorePacket("", ""));

      // ---- select_advancements_tab
      cb.accept("select_advancements_tab/none", new ClientboundSelectAdvancementsTabPacket(null));
      cb.accept("select_advancements_tab/tab", new ClientboundSelectAdvancementsTabPacket(id("minecraft:story/root")));
      cb.accept("select_advancements_tab/custom_ns", new ClientboundSelectAdvancementsTabPacket(id("my_pack:a/b/c.d-e_f")));

      // ---- server_data
      cb.accept("server_data/no_icon", new ClientboundServerDataPacket(plain, Optional.empty()));
      cb.accept("server_data/empty_icon", new ClientboundServerDataPacket(styled, Optional.of(new byte[0])));
      byte[] icon = new byte[1000];
      for (int i = 0; i < icon.length; i++) icon[i] = (byte) (i * 31);
      cb.accept("server_data/icon", new ClientboundServerDataPacket(Component.literal(NON_BMP), Optional.of(icon)));

      // ---- set_display_objective
      var sbd = new Scoreboard();
      var obj = sbd.addObjective("kills", ObjectiveCriteria.DUMMY, Component.literal("Kills"), ObjectiveCriteria.RenderType.INTEGER, true, null);
      for (var slot : DisplaySlot.values()) cb.accept("set_display_objective/" + slot.name(), new ClientboundSetDisplayObjectivePacket(slot, obj));
      cb.accept("set_display_objective/cleared", new ClientboundSetDisplayObjectivePacket(DisplaySlot.SIDEBAR, null));

      // ---- set_experience / set_health
      for (float[] v : new float[][] {{0f, 0, 0}, {0.5f, 100, 5}, {1f, Integer.MAX_VALUE, Integer.MAX_VALUE}, {Float.NaN, -1, -1}, {0.123f, 21863, 30}})
         cb.accept("set_experience/" + Float.floatToRawIntBits(v[0]) + "_" + (int) v[1], new ClientboundSetExperiencePacket(v[0], (int) v[1], (int) v[2]));
      for (float[] v : new float[][] {{20f, 20, 5f}, {0f, 0, 0f}, {0.5f, 1, 20.25f}, {Float.MAX_VALUE, -1, -3f}, {Float.NaN, Integer.MAX_VALUE, Float.NEGATIVE_INFINITY}})
         cb.accept("set_health/" + Float.floatToRawIntBits(v[0]) + "_" + (int) v[1], new ClientboundSetHealthPacket(v[0], (int) v[1], v[2]));

      // ---- set_objective
      NumberFormat[] formats = {null, BlankFormat.INSTANCE, StyledFormat.NO_STYLE, StyledFormat.SIDEBAR_DEFAULT, StyledFormat.PLAYER_LIST_DEFAULT,
         new StyledFormat(Style.EMPTY.withBold(true).withItalic(false).withUnderlined(true).withStrikethrough(true).withObfuscated(true).withColor(0x123456).withShadowColor(0x80ff00ff).withInsertion("ins").withFont(new FontDescription.Resource(id("minecraft:uniform")))),
         new FixedFormat(Component.literal("fixed")), new FixedFormat(styled), new FixedFormat(Component.empty())};
      int k = 0;
      for (var nf : formats) {
         for (var rt : ObjectiveCriteria.RenderType.values()) {
            var o = new Scoreboard().addObjective("o" + k, ObjectiveCriteria.DUMMY, k % 2 == 0 ? plain : styled, rt, true, nf);
            cb.accept("set_objective/add_" + k + "_" + rt.name(), new ClientboundSetObjectivePacket(o, 0));
            cb.accept("set_objective/change_" + k + "_" + rt.name(), new ClientboundSetObjectivePacket(o, 2));
         }
         k++;
      }
      var o0 = new Scoreboard().addObjective("removed", ObjectiveCriteria.DUMMY, styled, ObjectiveCriteria.RenderType.HEARTS, true, new FixedFormat(plain));
      cb.accept("set_objective/remove", new ClientboundSetObjectivePacket(o0, 1));
      cb.accept("set_objective/method_3", new ClientboundSetObjectivePacket(o0, 3));
      cb.accept("set_objective/method_127", new ClientboundSetObjectivePacket(o0, 127));
      cb.accept("set_objective/method_neg", new ClientboundSetObjectivePacket(o0, -1));

      // ---- set_score
      NumberFormat styledFmt = StyledFormat.SIDEBAR_DEFAULT;
      cb.accept("set_score/minimal", new ClientboundSetScorePacket("Steve", "kills", 0, Optional.empty(), Optional.empty()));
      cb.accept("set_score/display", new ClientboundSetScorePacket("Steve", "kills", 5, Optional.of(styled), Optional.empty()));
      cb.accept("set_score/format_blank", new ClientboundSetScorePacket("Alex", "deaths", -1, Optional.empty(), Optional.of(BlankFormat.INSTANCE)));
      cb.accept("set_score/format_styled", new ClientboundSetScorePacket("Alex", "deaths", 200, Optional.empty(), Optional.of(styledFmt)));
      cb.accept("set_score/all", new ClientboundSetScorePacket(NON_BMP, NON_BMP, Integer.MIN_VALUE, Optional.of(plain), Optional.of(new FixedFormat(styled))));
      cb.accept("set_score/max", new ClientboundSetScorePacket("#fake", "x", Integer.MAX_VALUE, Optional.of(Component.empty()), Optional.of(new FixedFormat(Component.empty()))));

      // ---- set_player_team
      var scoreboard = new Scoreboard();
      var t = scoreboard.addPlayerTeam("red_team");
      t.setDisplayName(styled);
      t.setPlayerPrefix(Component.literal("[R] "));
      t.setPlayerSuffix(Component.literal(NON_BMP));
      scoreboard.addPlayerToTeam("a", t);
      scoreboard.addPlayerToTeam("b", t);
      scoreboard.addPlayerToTeam("c", t);
      cb.accept("set_player_team/add_default", ClientboundSetPlayerTeamPacket.createAddOrModifyPacket(t, true));
      cb.accept("set_player_team/change_default", ClientboundSetPlayerTeamPacket.createAddOrModifyPacket(t, false));
      cb.accept("set_player_team/remove", ClientboundSetPlayerTeamPacket.createRemovePacket(t));
      cb.accept("set_player_team/join", ClientboundSetPlayerTeamPacket.createPlayerPacket(t, NON_BMP, ClientboundSetPlayerTeamPacket.Action.ADD));
      cb.accept("set_player_team/leave", ClientboundSetPlayerTeamPacket.createPlayerPacket(t, "b", ClientboundSetPlayerTeamPacket.Action.REMOVE));
      int i = 0;
      for (var color : TeamColor.values()) {
         var vis = Team.Visibility.values()[i % 4];
         var coll = Team.CollisionRule.values()[(i / 2) % 4];
         var params = new ClientboundSetPlayerTeamPacket.Parameters(i % 2 == 0 ? plain : styled, Component.literal("p" + i), Component.empty(), vis, coll, Optional.of(color), (byte) i);
         cb.accept("set_player_team/change_color_" + color.name(), team("t" + i, 2, params, List.of()));
         cb.accept("set_player_team/add_color_" + color.name(), team("t" + i, 0, params, i == 0 ? List.of() : List.of("p1", "p2")));
         i++;
      }
      for (var vis : Team.Visibility.values())
         for (var coll : Team.CollisionRule.values()) {
            var params = new ClientboundSetPlayerTeamPacket.Parameters(plain, plain, plain, vis, coll, Optional.empty(), (byte) -1);
            cb.accept("set_player_team/change_" + vis.name() + "_" + coll.name(), team("v", 2, params, List.of()));
         }
      cb.accept("set_player_team/add_many_players", team("big", 0, new ClientboundSetPlayerTeamPacket.Parameters(plain, plain, plain, Team.Visibility.ALWAYS, Team.CollisionRule.ALWAYS, Optional.empty(), (byte) 3), List.of("one", NON_BMP, "", "four", "five")));
      cb.accept("set_player_team/join_many", team("big", 3, null, List.of("x", "y")));
      cb.accept("set_player_team/leave_many", team("big", 4, null, List.of("x", "y", "z")));
      cb.accept("set_player_team/join_none", team("big", 3, null, List.of()));
      cb.accept("set_player_team/method_5", team("big", 5, null, List.of("ignored")));
      cb.accept("set_player_team/method_neg", team("big", -128, null, List.of("ignored")));
      cb.accept("set_player_team/remove_named", team(NON_BMP, 1, null, List.of("ignored")));

      // ---- tag_query
      var tag = new CompoundTag();
      tag.putInt("a", 1);
      tag.put("s", StringTag.valueOf(NON_BMP));
      var inner = new CompoundTag();
      inner.putLongArray("l", new long[] {1, -2, Long.MAX_VALUE});
      inner.putByteArray("b", new byte[] {1, 2, 3});
      inner.putIntArray("i", new int[] {});
      tag.put("inner", inner);
      cb.accept("tag_query/empty_compound", new ClientboundTagQueryPacket(0, new CompoundTag()));
      cb.accept("tag_query/compound", new ClientboundTagQueryPacket(300, tag));
      cb.accept("tag_query/null", new ClientboundTagQueryPacket(Integer.MAX_VALUE, null));
      cb.accept("tag_query/negative_id", new ClientboundTagQueryPacket(-1, tag));

      // ---- update_advancements
      var ts = java.time.Instant.ofEpochMilli(1700000000123L);
      var root = new AdvancementHolder(id("minecraft:story/root"), new Advancement(Optional.empty(), Optional.of(new DisplayInfo(new ItemStackTemplate(Items.GRASS_BLOCK), Component.translatable("advancements.story.root.title"), Component.translatable("advancements.story.root.description"), Optional.of(new ClientAsset.ResourceTexture(id("minecraft:gui/advancements/backgrounds/stone"))), AdvancementType.TASK, true, false, false)), AdvancementRewards.EMPTY, Map.of(), new AdvancementRequirements(List.of(List.of("crafting_table"))), false));
      var child = new AdvancementHolder(id("minecraft:story/mine_stone"), new Advancement(Optional.of(id("minecraft:story/root")), Optional.of(new DisplayInfo(new ItemStackTemplate(Items.WOODEN_PICKAXE, 1), plain, styled, Optional.empty(), AdvancementType.CHALLENGE, false, true, true)), AdvancementRewards.EMPTY, Map.of(), new AdvancementRequirements(List.of(List.of("a", "b"), List.of("c"))), true));
      var hidden = new AdvancementHolder(id("my_pack:x"), new Advancement(Optional.of(id("minecraft:story/root")), Optional.empty(), AdvancementRewards.EMPTY, Map.of(), AdvancementRequirements.EMPTY, false));
      var patch = DataComponentPatch.builder().set(DataComponents.CUSTOM_NAME, styled).set(DataComponents.MAX_STACK_SIZE, 16).build();
      var goal = new AdvancementHolder(id("minecraft:story/goal"), new Advancement(Optional.of(id("minecraft:story/root")), Optional.of(new DisplayInfo(new ItemStackTemplate(Items.DIAMOND.builtInRegistryHolder(), 64, patch), styled, Component.empty(), Optional.empty(), AdvancementType.GOAL, true, true, false)), AdvancementRewards.EMPTY, Map.of(), new AdvancementRequirements(List.of(List.of())), true));
      var all = new AdvancementHolder(id("minecraft:story/all_flags"), new Advancement(Optional.of(id("minecraft:story/root")), Optional.of(new DisplayInfo(new ItemStackTemplate(Items.STICK.builtInRegistryHolder(), 2, DataComponentPatch.EMPTY), plain, plain, Optional.empty(), AdvancementType.TASK, true, false, true)), AdvancementRewards.EMPTY, Map.of(), AdvancementRequirements.EMPTY, false));
      cb.accept("update_advancements/empty", new ClientboundUpdateAdvancementsPacket(false, List.of(), Set.of(), Map.of(), false));
      cb.accept("update_advancements/reset_show", new ClientboundUpdateAdvancementsPacket(true, List.of(), Set.of(), Map.of(), true));
      cb.accept("update_advancements/root", new ClientboundUpdateAdvancementsPacket(false, List.of(new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(root, 0f, 0f)), Set.of(), Map.of(), true));
      cb.accept("update_advancements/child", new ClientboundUpdateAdvancementsPacket(false, List.of(new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(child, 1.5f, -2.25f)), Set.of(), Map.of(), false));
      cb.accept("update_advancements/hidden", new ClientboundUpdateAdvancementsPacket(false, List.of(new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(hidden, Float.NaN, Float.MAX_VALUE)), Set.of(), Map.of(), false));
      cb.accept("update_advancements/goal_all", new ClientboundUpdateAdvancementsPacket(true, List.of(new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(goal, 3f, 4f), new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(all, -0.5f, 1e-10f)), Set.of(), Map.of(), true));
      var removed = new LinkedHashSet<Identifier>(List.of(id("minecraft:story/root"), id("my_pack:b/c"), id("minecraft:nether/x")));
      var prog = new LinkedHashMap<Identifier, AdvancementProgress>();
      prog.put(id("minecraft:story/root"), progress("crafting_table", ts));
      prog.put(id("minecraft:story/mine_stone"), progress("a", ts, "b", null, "c", java.time.Instant.ofEpochMilli(-1L)));
      prog.put(id("my_pack:empty"), progress());
      prog.put(id("my_pack:big"), progress(NON_BMP, java.time.Instant.ofEpochMilli(Long.MAX_VALUE), "z", java.time.Instant.ofEpochMilli(Long.MIN_VALUE)));
      cb.accept("update_advancements/removed", new ClientboundUpdateAdvancementsPacket(false, List.of(), removed, Map.of(), false));
      cb.accept("update_advancements/progress", new ClientboundUpdateAdvancementsPacket(false, List.of(), Set.of(), prog, false));
      cb.accept("update_advancements/everything", new ClientboundUpdateAdvancementsPacket(true, List.of(new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(root, 0f, 0f), new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(child, 1f, 0f), new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(goal, 2f, 0f)), removed, prog, true));
      var manyAdded = new ArrayList<ClientboundUpdateAdvancementsPacket.PositionedAdvancement>();
      for (int j = 0; j < 200; j++) manyAdded.add(new ClientboundUpdateAdvancementsPacket.PositionedAdvancement(new AdvancementHolder(id("big:adv" + j), new Advancement(Optional.empty(), Optional.empty(), AdvancementRewards.EMPTY, Map.of(), AdvancementRequirements.EMPTY, j % 2 == 0)), j, -j));
      cb.accept("update_advancements/many", new ClientboundUpdateAdvancementsPacket(false, manyAdded, Set.of(), Map.of(), false));
      PlayHarness.raw("display_info/standalone", buf -> DisplayInfo.STREAM_CODEC.encode(buf, child.value().display().get()));
      PlayHarness.raw("item_stack_template/diamond_patch", buf -> ItemStackTemplate.STREAM_CODEC.encode(buf, new ItemStackTemplate(Items.DIAMOND.builtInRegistryHolder(), 64, patch)));

      // ---- waypoint (TRACK/UNTRACK/UPDATE x every payload type)
      var iconDefault = new Waypoint.Icon();
      var iconStyle = icon("minecraft:bowtie", null);
      var iconColor = icon("minecraft:default", 0xFF123456);
      var iconBoth = icon("my_pack:fancy", 0xFFFFFFFF);
      var icons = new Waypoint.Icon[] {iconDefault, iconStyle, iconColor, iconBoth};
      String[] iconNames = {"default", "style", "color", "both"};
      for (int j = 0; j < icons.length; j++) {
         var ic = icons[j];
         String s = iconNames[j];
         cb.accept("waypoint/track_vec3i_" + s, ClientboundTrackedWaypointPacket.addWaypointPosition(U1, ic, new Vec3i(10, -64, -30000000)));
         cb.accept("waypoint/update_vec3i_" + s, ClientboundTrackedWaypointPacket.updateWaypointPosition(U2, ic, new Vec3i(Integer.MAX_VALUE, 320, Integer.MIN_VALUE)));
         cb.accept("waypoint/track_chunk_" + s, ClientboundTrackedWaypointPacket.addWaypointChunk(U1, ic, new ChunkPos(-5, 1000000)));
         cb.accept("waypoint/update_chunk_" + s, ClientboundTrackedWaypointPacket.updateWaypointChunk(U3, ic, new ChunkPos(0, 0)));
         cb.accept("waypoint/track_azimuth_" + s, ClientboundTrackedWaypointPacket.addWaypointAzimuth(U1, ic, 1.5707964f));
         cb.accept("waypoint/update_azimuth_" + s, ClientboundTrackedWaypointPacket.updateWaypointAzimuth(U2, ic, -3.1415927f));
      }
      cb.accept("waypoint/untrack", ClientboundTrackedWaypointPacket.removeWaypoint(U1));
      cb.accept("waypoint/untrack_zero", ClientboundTrackedWaypointPacket.removeWaypoint(U3));
      cb.accept("waypoint/azimuth_nan", ClientboundTrackedWaypointPacket.addWaypointAzimuth(U1, iconDefault, Float.NaN));
      // named (string) identifiers, every payload type, incl. an EMPTY waypoint that carries an icon
      cb.accept("waypoint/named_empty_icon", wpPacket("UPDATE", named("Steve", iconBoth, 0, b -> {})));
      cb.accept("waypoint/named_vec3i", wpPacket("TRACK", named(NON_BMP, iconStyle, 1, b -> { b.writeVarInt(1); b.writeVarInt(2); b.writeVarInt(3); })));
      cb.accept("waypoint/named_chunk", wpPacket("TRACK", named("", iconColor, 2, b -> { b.writeVarInt(-1); b.writeVarInt(-2); })));
      cb.accept("waypoint/named_azimuth", wpPacket("UNTRACK", named("n", iconDefault, 3, b -> b.writeFloat(0.5f))));

      // ---- serverbound
      sb.accept("seen_advancements/opened_tab", ServerboundSeenAdvancementsPacket.openedTab(root));
      sb.accept("seen_advancements/opened_tab_custom", new ServerboundSeenAdvancementsPacket(ServerboundSeenAdvancementsPacket.Action.OPENED_TAB, id("my_pack:a/b")));
      sb.accept("seen_advancements/closed_screen", ServerboundSeenAdvancementsPacket.closedScreen());
      for (int[] v : new int[][] {{0, 0, 0, 0}, {1, 1, 1, 1}, {300, 5, 70, 1}, {Integer.MAX_VALUE, 0, 0, 0}, {-1, 0, 0, 0}})
         sb.accept("entity_tag_query/" + v[0], new ServerboundEntityTagQueryPacket(v[0], v[1] == 5 ? 123456 : -v[1]));
      for (var pos : new BlockPos[] {BlockPos.ZERO, new BlockPos(1, 2, 3), new BlockPos(-30000000, -2048, 29999999), new BlockPos(33554431, 2047, -33554432)})
         sb.accept("block_entity_tag_query/" + pos.getX() + "_" + pos.getY(), new ServerboundBlockEntityTagQueryPacket(pos.getX() & 1023, pos));

      PlayHarness.close();
   }
}
