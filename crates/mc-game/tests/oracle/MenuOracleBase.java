import com.mojang.serialization.DynamicOps;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.MapPostProcessing;
import net.minecraft.world.item.trading.ItemCost;
import net.minecraft.world.item.trading.MerchantOffers;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.TagParser;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.equine.AbstractHorse;
import net.minecraft.world.entity.npc.villager.AbstractVillager;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.stats.Stats;
import net.minecraft.world.MenuProvider;
import net.minecraft.world.entity.ExperienceOrb;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.EnchantingTableBlock;
import net.minecraft.world.level.block.HorizontalDirectionalBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.saveddata.maps.MapId;
import net.minecraft.world.level.saveddata.maps.MapItemSavedData;
import net.minecraft.world.phys.AABB;

/**
 * In-process menu oracle: scripted sessions through the REAL vanilla menus of a real ServerPlayer on a real ServerLevel
 * (VanillaLevel, void world). The menu is opened the way a player does it: the block at TABLE provides it
 * (BlockState.getMenuProvider + ServerPlayer.openMenu). The Rust twin is tests/golden/common/menu_oracle.rs.
 * Usage: scripts/menu-oracle.sh NAME   (runs oracle/menu_scripts/NAME.txt)
 * Output: crates/mc-game/tests/fixtures/menu_oracle/NAME.txt.gz (the wrapper compresses stdout)
 *        other oracles (EnchantAnvilOracle) call boot(), the op methods and `script(...)` directly.
 *
 * Script lines (stacks are `@item[components]` (the command syntax, no spaces, count 1) or `name[xCOUNT][,d=damage][,rc=repairCost][,n=custom name][,e=ench@lvl;..][,s=stored ench@lvl;..][,m=map id][,b=pattern@color;..][,p=potion]`;
 * the printed ops carry the stack as `ItemStack.CODEC` SNBT, `-` = empty); `#` lines are copied:
 *   OPEN kind [xp [seed [creative [pseed [lseed [shelves [blockers]]]]]]]
 *        kind from KINDS (enchant | anvil | chipped_anvil | damaged_anvil | grindstone | loom | crafting | chest | cartography | brewing_stand | beacon | lectern | stonecutter | crafter | smithing | furnace | smoker | blast_furnace): resets the
 *        world around TABLE, places the kind's block and opens the menu it provides. xp = experience level, seed = enchantment seed, creative 0|1,
 *        pseed/lseed = seeds of the player's and the level's random, shelves/blockers = bookshelves/obstructions of an enchanting table,
 *        the levels (0..4) of the iron pyramid under a beacon, the pages (at least 1) of the book on a lectern and whether that book is a writable one.
 *        Gravity blocks (anvils) fall when ticked: no support block is placed.
 *   SET slot STACK | GIVE slot STACK    menu slot / player inventory slot (menu.broadcastChanges afterwards)
 *   CLICK slot button INPUT             menu.clicked in sync (suppressRemoteUpdates, clicked, resumeRemoteUpdates, broadcastChanges)
 *   DRAG type slot...                   the three CLICK groups of a quick-craft drag (type 0 charitable | 1 greedy | 2 clone)
 *   BUTTON id | NAME text | CLOSE
 *   BLOCK x y z id[props]               places a block (and its block entity) without resetting anything; undone by the next OPEN
 *   SLOTSTATE slot 0|1                  the slot-state packet of the crafter menu (1 = enabled): the crafter's setSlotState, then broadcastChanges
 *   TICK n                              n real server ticks (the player's own tick: broadcastChanges, close when stillValid fails);
 *                                       ticks consume the level's random in a server dependent way: SEED level before the next draw.
 *                                       The stand-in player of the Rust twin ticks after the level (vanilla: before the block entities), so the
 *                                       oracle broadcasts the menu once more after the ticks: block entity changes are sent in the TICK op
 *   RECIPE give|take id                 `/recipe give|take` for the player; `*` = every recipe
 *   GAMERULE name value | SEED player|level|enchant n | MAP id x z scale locked
 *   EVENTS on|off                       print the EV lines of every PlayerList.broadcast since the last op (see VanillaLevel.RecordingPlayerList)
 *   MAYBUILD 0|1                        `abilities.mayBuild` of the player (reset by OPEN)
 *   SETBEACON primary secondary         handleSetBeaconPacket on the open beacon menu (effect path or `-`); prints `R true|false|npe` = updateEffects (false: the listener
 *                                       disconnects; npe: validateEffects throws, the packet processor suppresses it) or `R skipped` when stillValid fails
 *   GEN kind seed n spec|spec|.. [ticks]  n random ops (SET/GIVE/CLICK/DRAG/BUTTON) on a fresh `kind` session, from java.util.Random(seed);
 *                                       `ticks` replaces half of the BUTTON ops by TICK 1..30 and never drops items (no throw, no click outside the window:
 *                                       the real player would pick them up while ticking, the replay's stand-in does not)
 *   GENCONT seed n spec|spec|..         n random ops on the menu that is open now (TRADE replaces BUTTON in a trading screen), no OPEN and no CLOSE
 *   Entity menus: `OPEN merchant|mount ..` only resets (no block, no menu); the menu comes from an entity:
 *   SPAWN ref type x y z seed [SNBT]    the entity (no spaces in the SNBT), its random seeded with `seed`; discarded by the next OPEN
 *   USE ref [sneak]                     a horse opens its inventory (`openCustomInventoryScreen`), another entity is interacted with (a villager starts trading)
 *   TRADE n                             `handleSelectTrade`: selection hint and `tryMoveItems`        NBT ref SNBT   merge SNBT into the entity (`/data merge`)
 *   DISCARD ref | PLAYER x y z | HEALTH n|disconnected     remove the entity | `setPos` of the player | its health (0 = dead, no death processing) or its disconnected flag
 * Stdout: one "OP ..." line per op (replayed by Rust) followed by "OUT ..." lines, only those that changed since the last printed
 * line of the same kind (reset by OPEN):
 *   OUT S i=stack ..   menu slots (non-empty)   OUT D d0 d1 ..  data slots   OUT C carried   OUT I i=stack  player inventory
 *   OUT X level seed totalXp progressBits   OUT B state of the block at TABLE (+ has_bottle_N, has_book, powered when it has them)   OUT ORBS n  experience of the orbs near TABLE
 *   OUT DROPS id=n,..  dropped item entities (only when there are some)   OUT K ids  recipes the player knows (after a RECIPE op)
 *   OUT M id=cx,cz,scale,locked,colorHash ..  every map of the level, ids 0..63 (only when any)   OUT R b  result of BUTTON / NAME
 *   OUT CRAFTED id=n ..  the `item_crafted` statistics of the player (only when any)
 *   OUT PKT ...  container packets sent since the last op   OUT EV ...  broadcasts (EVENTS on)
 *   OUT P x,y,z alive  the player once it moved or died   OUT E ref alive=A [xp= trading= offers=uses/max/demand/special;..] [persist= drops=]  entities of SPAWN
 */
public class MenuOracleBase {
   static final PrintStream OUT = VanillaLevel.OUT;
   static VanillaLevel v;
   static ServerLevel level;
   static VanillaLevel.FakePlayer fp;
   static ServerPlayer p;
   static DynamicOps<Tag> ops;
   static final BlockPos TABLE = new BlockPos(0, 70, 0);
   static AbstractContainerMenu menu;
   static String kind;
   static boolean events;
   /** Set by the first RECIPE op: the real player also unlocks recipes by itself (inventory changes), which is only compared once scripts look at recipes. */
   static boolean recipes;
   static final List<BlockPos> placed = new ArrayList<>();
   /** The map ids 0..MAP_PROBE the M line looks at: the ones made by MAP and the ones crafted (cartography table). */
   static final int MAP_PROBE = 64;
   /** Entities of SPAWN by their names; discarded by the next OPEN. */
   static final Map<String, Entity> ents = new LinkedHashMap<>();

   /** What a kind adds around TABLE besides its block (shelves/blockers of an enchanting table). */
   interface Setup {
      void run(int shelves, int blockers);
   }

   /** The block at TABLE that provides the menu of a kind. */
   record Kind(Block block, Setup setup) {
   }

   static final Setup NO_SETUP = (shelves, blockers) -> { };
   static final Map<String, Kind> KINDS = new LinkedHashMap<>();
   /** Block registries only exist after the bootstrap: filled by boot(). */
   static void registerKinds() {
      KINDS.put("enchant", new Kind(Blocks.ENCHANTING_TABLE, (shelves, blockers) -> {
         List<BlockPos> offsets = EnchantingTableBlock.BOOKSHELF_OFFSETS;
         for (int i = 0; i < shelves && i < offsets.size(); i++) put(TABLE.offset(offsets.get(i)), Blocks.BOOKSHELF.defaultBlockState());
         for (int i = 0; i < blockers && i < offsets.size(); i++) {
            BlockPos o = offsets.get(i);
            put(TABLE.offset(o.getX() / 2, o.getY(), o.getZ() / 2), Blocks.STONE.defaultBlockState());
         }
      }));
      KINDS.put("anvil", new Kind(Blocks.ANVIL, NO_SETUP));
      KINDS.put("chipped_anvil", new Kind(Blocks.CHIPPED_ANVIL, NO_SETUP));
      KINDS.put("damaged_anvil", new Kind(Blocks.DAMAGED_ANVIL, NO_SETUP));
      KINDS.put("grindstone", new Kind(Blocks.GRINDSTONE, NO_SETUP));
      KINDS.put("loom", new Kind(Blocks.LOOM, NO_SETUP));
      KINDS.put("crafting", new Kind(Blocks.CRAFTING_TABLE, NO_SETUP));
      KINDS.put("chest", new Kind(Blocks.CHEST, NO_SETUP));
      KINDS.put("cartography", new Kind(Blocks.CARTOGRAPHY_TABLE, NO_SETUP));
      KINDS.put("brewing_stand", new Kind(Blocks.BREWING_STAND, NO_SETUP));
      KINDS.put("crafter", new Kind(Blocks.CRAFTER, NO_SETUP));
      // shelves = levels of the iron pyramid under the beacon
      KINDS.put("beacon", new Kind(Blocks.BEACON, (levels, unused) -> {
         for (int i = 1; i <= Math.min(levels, 4); i++) {
            for (int dx = -i; dx <= i; dx++) for (int dz = -i; dz <= i; dz++) put(TABLE.offset(dx, -i, dz), Blocks.IRON_BLOCK.defaultBlockState());
         }
      }));
      // shelves = pages (>= 1) of the book, blockers != 0: a writable book instead of a written one
      KINDS.put("lectern", new Kind(Blocks.LECTERN, (pages, writable) -> {
         put(TABLE, Blocks.LECTERN.defaultBlockState().setValue(net.minecraft.world.level.block.LecternBlock.HAS_BOOK, true));
         ((net.minecraft.world.level.block.entity.LecternBlockEntity) level.getBlockEntity(TABLE)).setBook(book(pages, writable != 0));
      }));
      KINDS.put("stonecutter", new Kind(Blocks.STONECUTTER, NO_SETUP));
      KINDS.put("smithing", new Kind(Blocks.SMITHING_TABLE, NO_SETUP));
      // no block: the menu comes from an entity (SPAWN, USE)
      KINDS.put("merchant", new Kind(null, NO_SETUP));
      KINDS.put("mount", new Kind(null, NO_SETUP));
      KINDS.put("furnace", new Kind(Blocks.FURNACE, NO_SETUP));
      KINDS.put("smoker", new Kind(Blocks.SMOKER, NO_SETUP));
      KINDS.put("blast_furnace", new Kind(Blocks.BLAST_FURNACE, NO_SETUP));
   }

   static String st(ItemStack s) {
      if (s.isEmpty()) return "-";
      return ItemStack.CODEC.encodeStart(ops, s).getOrThrow().toString();
   }

   static void op(String s) { OUT.println("OP " + s); }
   static void out(String s) { OUT.println("OUT " + s); }
   /** State lines are only printed when they differ from the last printed line of the same kind (reset by OPEN). */
   static final Map<String, String> prev = new HashMap<>();
   static void state(String key, String line) {
      if (!line.equals(prev.put(key, line))) out(line);
   }
   /** Like {@link #state} but the `empty` line counts as already printed: a session that never uses the feature prints nothing. */
   static void stateUnlessEmpty(String key, String line, String empty) {
      if (!line.equals(prev.getOrDefault(key, empty))) {
         prev.put(key, line);
         out(line);
      }
   }

   static Holder<Enchantment> ench(String id) {
      return level.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(ResourceKey.create(Registries.ENCHANTMENT, Identifier.withDefaultNamespace(id)));
   }

   static Item item(String id) { return BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(id)); }

   /** name[count][,d=damage][,rc=repairCost][,n=custom name][,e=ench@lvl;..][,s=stored ench@lvl;..][,m=map id][,b=pattern@color;..] */
   static ItemStack mk(String spec) {
      if (spec.equals("-")) return ItemStack.EMPTY;
      if (spec.startsWith("@")) {
         try {
            return new net.minecraft.commands.arguments.item.ItemParser(level.registryAccess()).parse(new com.mojang.brigadier.StringReader(spec.substring(1))).createItemStack(1);
         } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
            throw new IllegalArgumentException(spec, e);
         }
      }
      String[] parts = spec.split(",");
      String head = parts[0];
      int count = 1;
      java.util.regex.Matcher m = java.util.regex.Pattern.compile("^(.*?)x(\\d+)$").matcher(head);
      if (m.matches()) { head = m.group(1); count = Integer.parseInt(m.group(2)); }
      ItemStack s = new ItemStack(item(head), count);
      if (s.isEmpty()) throw new IllegalArgumentException("unknown item " + head);
      for (int i = 1; i < parts.length; i++) {
         String k = parts[i].substring(0, parts[i].indexOf('=')), val = parts[i].substring(parts[i].indexOf('=') + 1);
         switch (k) {
            case "d" -> s.setDamageValue(Integer.parseInt(val));
            case "rc" -> s.set(DataComponents.REPAIR_COST, Integer.parseInt(val));
            case "n" -> s.set(DataComponents.CUSTOM_NAME, Component.literal(val));
            case "e", "s" -> { for (String e : val.split(";")) { String[] x = e.split("@"); s.enchant(ench(x[0]), Integer.parseInt(x[1])); } }
            case "m" -> s.set(DataComponents.MAP_ID, new MapId(Integer.parseInt(val)));
            case "b" -> {
               var layers = new net.minecraft.world.level.block.entity.BannerPatternLayers.Builder();
               for (String e : val.split(";")) {
                  String[] x = e.split("@");
                  layers.add(level.registryAccess().lookupOrThrow(Registries.BANNER_PATTERN)
                     .getOrThrow(ResourceKey.create(Registries.BANNER_PATTERN, Identifier.withDefaultNamespace(x[0]))),
                     net.minecraft.world.item.DyeColor.valueOf(x[1].toUpperCase()));
               }
               s.set(DataComponents.BANNER_PATTERNS, layers.build());
            }
            case "p" -> s.set(DataComponents.POTION_CONTENTS, new net.minecraft.world.item.alchemy.PotionContents(
               level.registryAccess().lookupOrThrow(Registries.POTION).getOrThrow(ResourceKey.create(Registries.POTION, Identifier.withDefaultNamespace(val)))));
            default -> throw new IllegalArgumentException(k);
         }
      }
      return s;
   }

   static void setField(Class<?> c, Object o, String f, Object val) throws Exception {
      Field fld = c.getDeclaredField(f);
      fld.setAccessible(true);
      fld.set(o, val);
   }

   static List<String> packetLines() {
      List<String> lines = new ArrayList<>();
      for (VanillaLevel.Sent s : fp.drain()) {
         Object pk = s.packet();
         if (pk instanceof ClientboundOpenScreenPacket o) lines.add("PKT open " + o.getContainerId() + " " + BuiltInRegistries.MENU.getKey(o.getType()).getPath());
         else if (pk instanceof ClientboundContainerSetContentPacket c) {
            StringBuilder b = new StringBuilder("PKT content " + c.containerId() + " " + c.stateId() + " carried=" + st(c.carriedItem()));
            for (int i = 0; i < c.items().size(); i++) if (!c.items().get(i).isEmpty()) b.append(' ').append(i).append('=').append(st(c.items().get(i)));
            lines.add(b.toString());
         } else if (pk instanceof ClientboundContainerSetSlotPacket c) lines.add("PKT slot " + c.getContainerId() + " " + c.getStateId() + " " + c.getSlot() + " " + st(c.getItem()));
         else if (pk instanceof ClientboundContainerSetDataPacket c) lines.add("PKT data " + c.getContainerId() + " " + c.getId() + " " + c.getValue());
         else if (pk instanceof ClientboundSetCursorItemPacket c) lines.add("PKT cursor " + st(c.contents()));
         else if (pk instanceof ClientboundContainerClosePacket c) lines.add("PKT close " + c.getContainerId());
      }
      return lines;
   }

   @SuppressWarnings("unchecked")
   static void dump() {
      boolean closed = menu == p.inventoryMenu;
      StringBuilder b = new StringBuilder("S");
      for (int i = 0; i < menu.slots.size(); i++) {
         ItemStack s = menu.slots.get(i).getItem();
         if (!s.isEmpty()) b.append(' ').append(i).append('=').append(st(s));
      }
      if (!closed) state("S", b.toString());
      StringBuilder d = new StringBuilder("D");
      try {
         Field f = AbstractContainerMenu.class.getDeclaredField("dataSlots");
         f.setAccessible(true);
         for (DataSlot ds : (List<DataSlot>) f.get(menu)) d.append(' ').append(ds.get());
      } catch (Exception e) { throw new RuntimeException(e); }
      if (!closed) {
         state("D", d.toString());
         state("C", "C " + st(menu.getCarried()));
      }
      StringBuilder inv = new StringBuilder("I");
      for (int i = 0; i < p.getInventory().getContainerSize(); i++) {
         ItemStack s = p.getInventory().getItem(i);
         if (!s.isEmpty()) inv.append(' ').append(i).append('=').append(st(s));
      }
      state("I", inv.toString());
      state("X", "X " + p.experienceLevel + " " + p.getEnchantmentSeed() + " " + p.totalExperience + " " + Float.floatToRawIntBits(p.experienceProgress));
      BlockState bs = level.getBlockState(TABLE);
      StringBuilder bstate = new StringBuilder("B " + bs.getBlock().builtInRegistryHolder().key().identifier().getPath() + (bs.hasProperty(HorizontalDirectionalBlock.FACING) ? " " + bs.getValue(HorizontalDirectionalBlock.FACING) : ""));
      for (String name : List.of("has_bottle_0", "has_bottle_1", "has_bottle_2", "has_book", "powered")) {
         for (var prop : bs.getProperties()) if (prop.getName().equals(name)) bstate.append(' ').append(name).append('=').append(bs.getValue(prop));
      }
      state("B", bstate.toString());
      List<ExperienceOrb> orbs = level.getEntitiesOfClass(ExperienceOrb.class, new AABB(TABLE).inflate(3.0));
      // orb merging depends on entity ids (not comparable across servers): compare the total experience
      int total = 0;
      try {
         Field cf = ExperienceOrb.class.getDeclaredField("count");
         cf.setAccessible(true);
         for (ExperienceOrb orb : orbs) total += orb.getValue() * cf.getInt(orb);
      } catch (Exception e) { throw new RuntimeException(e); }
      state("ORBS", "ORBS " + total);
      // item entities merge in an order that depends on entity ids: compare the totals per item
      Map<String, Integer> drops = new TreeMap<>();
      for (ItemEntity e : level.getEntitiesOfClass(ItemEntity.class, new AABB(TABLE).inflate(8.0))) {
         drops.merge(BuiltInRegistries.ITEM.getKey(e.getItem().getItem()).toString(), e.getItem().getCount(), Integer::sum);
      }
      stateUnlessEmpty("DROPS", "DROPS" + drops.entrySet().stream().map(e -> " " + e.getKey() + "=" + e.getValue()).reduce("", String::concat), "DROPS");
      if (recipes) {
         TreeSet<String> known = new TreeSet<>();
         p.getRecipeBook().pack().known().forEach(k -> known.add(k.identifier().toString()));
         stateUnlessEmpty("K", "K" + known.stream().map(k -> " " + k).reduce("", String::concat), "K");
      }
      TreeMap<String, Integer> crafted = new TreeMap<>();
      for (Item it : BuiltInRegistries.ITEM) {
         int n = p.getStats().getValue(Stats.ITEM_CRAFTED.get(it));
         if (n > 0) crafted.put(BuiltInRegistries.ITEM.getKey(it).toString(), n);
      }
      stateUnlessEmpty("CRAFTED", "CRAFTED" + crafted.entrySet().stream().map(e -> " " + e.getKey() + "=" + e.getValue()).reduce("", String::concat), "CRAFTED");
      StringBuilder m = new StringBuilder("M");
      for (int id = 0; id < MAP_PROBE; id++) {
         MapItemSavedData data = level.getMapData(new MapId(id));
         if (data != null) m.append(' ').append(id).append('=').append(data.centerX).append(',').append(data.centerZ).append(',').append(data.scale)
            .append(',').append(data.locked ? 1 : 0).append(',').append(Arrays.hashCode(data.colors));
      }
      stateUnlessEmpty("M", m.toString(), "M");
      // the player's position and life only show once an op moved or killed it
      stateUnlessEmpty("P", "P " + p.getX() + "," + p.getY() + "," + p.getZ() + " " + (p.isAlive() ? 1 : 0), "P 0.5,71.0,0.5 1");
      ents.forEach((ref, e) -> state("E" + ref, entityLine(ref, e)));
      if (events) for (String e : VanillaLevel.EVENTS) out(e);
      VanillaLevel.EVENTS.clear();
      for (String l : packetLines()) out(l);
   }

   /** `E ref alive=A` plus `xp= trading= offers=uses/max/demand/special;..` of a villager and `persist= drops=` of a mob. */
   static String entityLine(String ref, Entity e) {
      StringBuilder b = new StringBuilder("E " + ref + " alive=" + (e.isAlive() ? 1 : 0));
      if (!e.isAlive()) return b.toString();
      if (e instanceof AbstractVillager av) {
         b.append(" xp=").append(av.getVillagerXp()).append(" trading=").append(av.getTradingPlayer() != null ? 1 : 0).append(" offers=");
         StringJoiner offers = new StringJoiner(";");
         for (MerchantOffer o : av.getOffers()) offers.add(o.getUses() + "/" + o.getMaxUses() + "/" + o.getDemand() + "/" + o.getSpecialPriceDiff());
         b.append(offers);
      }
      if (e instanceof AbstractHorse) {
         try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
            TagValueOutput o = TagValueOutput.createWithContext(rep, level.registryAccess());
            e.saveWithoutId(o);
            CompoundTag tag = o.buildResult();
            b.append(" persist=").append(tag.getBooleanOr("PersistenceRequired", false) ? 1 : 0)
               .append(" drops=").append(tag.get("drop_chances") == null ? "-" : EntityOracleDriver.canon(tag.get("drop_chances")));
         }
      }
      return b.toString();
   }

   /** `SPAWN ref type x y z seed [SNBT]`: the entity as the driver's `spawn` makes it, its random seeded with `seed`. */
   static void spawn(String ref, String type, double x, double y, double z, long seed, String snbt) throws Exception {
      op("SPAWN " + ref + " " + type + " " + x + " " + y + " " + z + " " + seed + (snbt == null ? "" : " " + snbt));
      Entity e = BuiltInRegistries.ENTITY_TYPE.getValue(Identifier.parse(type.contains(":") ? type : "minecraft:" + type)).create(level, EntitySpawnReason.COMMAND);
      e.snapTo(x, y, z, 0f, 0f);
      if (snbt != null) {
         try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
            TagValueOutput o = TagValueOutput.createWithContext(rep, level.registryAccess());
            e.saveWithoutId(o);
            CompoundTag merged = o.buildResult().merge(TagParser.parseCompoundFully(snbt));
            e.load(TagValueInput.create(rep, level.registryAccess(), merged));
         }
      }
      e.getRandom().setSeed(seed);
      level.addFreshEntity(e);
      ents.put(ref, e);
      dump();
   }

   /** `USE ref [sneak]`: a horse opens its inventory screen, anything else is interacted with (a villager starts trading). */
   static void use(String ref, boolean sneak) {
      op("USE " + ref + " " + (sneak ? 1 : 0));
      Entity e = ents.get(ref);
      p.setShiftKeyDown(sneak);
      if (e instanceof AbstractHorse h) h.openCustomInventoryScreen(p);
      else e.interact(p, InteractionHand.MAIN_HAND, e.position());
      p.setShiftKeyDown(false);
      menu = p.containerMenu;
      dump();
   }

   /** `TRADE n`: `ServerGamePacketListenerImpl.handleSelectTrade`. */
   static void trade(int selection) {
      op("TRADE " + selection);
      if (p.containerMenu instanceof MerchantMenu m && m.stillValid(p)) {
         m.setSelectionHint(selection);
         m.tryMoveItems(selection);
      }
      menu.broadcastChanges();
      dump();
   }

   /** `NBT ref SNBT`: merges SNBT into an entity of SPAWN (the driver's `nbt`). */
   static void nbt(String ref, String snbt) {
      op("NBT " + ref + " " + snbt);
      Entity e = ents.get(ref);
      try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
         TagValueOutput o = TagValueOutput.createWithContext(rep, level.registryAccess());
         e.saveWithoutId(o);
         CompoundTag merged = o.buildResult().merge(TagParser.parseCompoundFully(snbt));
         e.load(TagValueInput.create(rep, level.registryAccess(), merged));
      } catch (Exception ex) {
         throw new RuntimeException(ex);
      }
      dump();
   }

   /** `DISCARD ref`: the entity leaves the level. */
   static void discard(String ref) {
      op("DISCARD " + ref);
      ents.get(ref).discard();
      dump();
   }

   /** `PLAYER x y z`: `setPos` of the player. */
   static void player(double x, double y, double z) {
      op("PLAYER " + x + " " + y + " " + z);
      p.setPos(x, y, z);
      dump();
   }

   /** `HEALTH n`: the player's health (0 = dead without the death processing) or `DISCONNECTED`. */
   static void health(String what) throws Exception {
      op("HEALTH " + what);
      if (what.equals("disconnected")) setField(ServerPlayer.class, p, "disconnected", true);
      else p.setHealth(Float.parseFloat(what));
      dump();
   }

   /**
    * `MAPOFFERS ref`: the villager's offers become two cartographer offers (13 and 14 emeralds with a compass) whose result is a map that
    * the trade finishes when it is taken: locked (id 5) or scaled (id 6). Map post-processing is not stored in the Offers NBT.
    */
   static void mapOffers(String ref) throws Exception {
      op("MAPOFFERS " + ref);
      MerchantOffers offers = new MerchantOffers();
      for (int i = 0; i < 2; i++) {
         ItemStack map = new ItemStack(Items.FILLED_MAP);
         map.set(DataComponents.MAP_ID, new MapId(5 + i));
         map.set(DataComponents.MAP_POST_PROCESSING, i == 0 ? MapPostProcessing.LOCK : MapPostProcessing.SCALE);
         offers.add(new MerchantOffer(new ItemCost(Items.EMERALD, 13 + i), Optional.of(new ItemCost(Items.COMPASS, 1)), map, 12, 1, 0.05F));
      }
      setField(AbstractVillager.class, ents.get(ref), "offers", offers);
      dump();
   }

   static void put(BlockPos pos, BlockState state) {
      level.setBlock(pos, state, 3);
      placed.add(pos);
   }

   static void clearArea() {
      for (BlockPos pos : placed) level.setBlock(pos, Blocks.AIR.defaultBlockState(), 3);
      placed.clear();
      for (int x = -4; x <= 4; x++) for (int y = -2; y <= 3; y++) for (int z = -4; z <= 4; z++) level.setBlock(TABLE.offset(x, y, z), Blocks.AIR.defaultBlockState(), 3);
      // removing a container drops its items: after the blocks
      for (ExperienceOrb orb : new ArrayList<>(level.getEntitiesOfClass(ExperienceOrb.class, new AABB(TABLE).inflate(8.0)))) orb.discard();
      for (ItemEntity e : new ArrayList<>(level.getEntitiesOfClass(ItemEntity.class, new AABB(TABLE).inflate(8.0)))) e.discard();
   }

   static void open(String k, int xp, int seed, boolean creative, long pseed, long lseed, int shelves, int blockers) throws Exception {
      Kind spec = KINDS.get(k);
      if (spec == null) throw new IllegalArgumentException("unknown kind " + k + " (" + KINDS.keySet() + ")");
      kind = k;
      prev.clear();
      VanillaLevel.EVENTS.clear();
      op("OPEN " + k + " " + xp + " " + seed + " " + (creative ? 1 : 0) + " " + pseed + " " + lseed + " " + shelves + " " + blockers);
      if (p.containerMenu != p.inventoryMenu) p.doCloseContainer();
      for (int i = 0; i < p.getInventory().getContainerSize(); i++) p.getInventory().setItem(i, ItemStack.EMPTY);
      p.inventoryMenu.setCarried(ItemStack.EMPTY);
      if (recipes && !p.getRecipeBook().pack().known().isEmpty()) v.command("recipe take Steve *");
      for (Entity e : ents.values()) e.discard();
      ents.clear();
      clearArea();
      p.setPos(0.5, 71, 0.5);
      p.setHealth(p.getMaxHealth());
      p.setShiftKeyDown(false);
      setField(ServerPlayer.class, p, "disconnected", false);
      p.experienceLevel = xp;
      p.totalExperience = 0;
      p.experienceProgress = 0;
      setField(Player.class, p, "enchantmentSeed", seed);
      p.getAbilities().instabuild = creative;
      p.getAbilities().mayBuild = true;
      p.getRandom().setSeed(pseed);
      level.getRandom().setSeed(lseed);
      if (spec.block() != null) {
         put(TABLE, spec.block().defaultBlockState());
         spec.setup().run(shelves, blockers);
         MenuProvider provider = level.getBlockState(TABLE).getMenuProvider(level, TABLE);
         if (provider != null) p.openMenu(provider);
      }
      menu = p.containerMenu;
      dump();
   }

   static void set(int slot, ItemStack stack) {
      op("SET " + slot + " " + st(stack));
      menu.getSlot(slot).set(stack);
      menu.broadcastChanges();
      dump();
   }

   static void give(int slot, ItemStack stack) {
      op("GIVE " + slot + " " + st(stack));
      p.getInventory().setItem(slot, stack);
      menu.broadcastChanges();
      dump();
   }

   static void click(int slot, int button, ContainerInput input) {
      op("CLICK " + slot + " " + button + " " + input.name());
      // ServerGamePacketListenerImpl.handleContainerClick for an in-sync client
      menu.suppressRemoteUpdates();
      menu.clicked(slot, button, input, p);
      menu.resumeRemoteUpdates();
      menu.broadcastChanges();
      dump();
   }

   /** The three click groups of a drag over `slots` (ServerboundContainerClickPacket headers -999 / slot / -999). */
   static void drag(int type, int... slots) {
      click(-999, AbstractContainerMenu.getQuickcraftMask(0, type), ContainerInput.QUICK_CRAFT);
      for (int s : slots) click(s, AbstractContainerMenu.getQuickcraftMask(1, type), ContainerInput.QUICK_CRAFT);
      click(-999, AbstractContainerMenu.getQuickcraftMask(2, type), ContainerInput.QUICK_CRAFT);
   }

   static void button(int id) {
      op("BUTTON " + id);
      boolean r = menu.clickMenuButton(p, id);
      if (r) menu.broadcastChanges();
      out("R " + r);
      dump();
   }

   static void name(String name) {
      op("NAME " + name);
      if (menu instanceof AnvilMenu anvil) {
         boolean r = anvil.setItemName(name);
         out("R " + r);
      }
      dump();
   }

   static void close() {
      op("CLOSE");
      p.doCloseContainer();
      menu = p.containerMenu;
      dump();
   }

   static void block(int x, int y, int z, String id) throws Exception {
      String full = id.contains(":") ? id : "minecraft:" + id;
      op("BLOCK " + x + " " + y + " " + z + " " + full);
      put(new BlockPos(x, y, z), BlockStateParser.parseForBlock(level.holderLookup(Registries.BLOCK), full, false).blockState());
      dump();
   }

   static void tick(int n) {
      op("TICK " + n);
      for (int i = 0; i < n; i++) v.tick();
      if (p.containerMenu != p.inventoryMenu) p.containerMenu.broadcastChanges();
      menu = p.containerMenu;
      dump();
   }

   static void recipe(String what, String id) {
      op("RECIPE " + what + " " + id);
      recipes = true;
      v.command("recipe " + what + " Steve " + id);
      v.commandOutput.clear();
      dump();
   }

   static void gamerule(String name, String value) {
      op("GAMERULE " + name + " " + value);
      v.command("gamerule " + name + " " + value);
      v.commandOutput.clear();
      dump();
   }

   static void seed(String which, long value) throws Exception {
      op("SEED " + which + " " + value);
      switch (which) {
         case "player" -> p.getRandom().setSeed(value);
         case "level" -> level.getRandom().setSeed(value);
         case "enchant" -> setField(Player.class, p, "enchantmentSeed", (int) value);
         default -> throw new IllegalArgumentException("SEED player|level|enchant, got " + which);
      }
      dump();
   }

   static void map(int id, int x, int z, int scale, boolean locked) {
      op("MAP " + id + " " + x + " " + z + " " + scale + " " + (locked ? 1 : 0));
      MapItemSavedData data = MapItemSavedData.createFresh(x, z, (byte) scale, false, false, level.dimension());
      if (locked) data = data.locked();
      level.setMapData(new MapId(id), data);
      dump();
   }

   /** The book of a lectern: `pages` pages "page N" (SNBT shared with the Rust twin). */
   static ItemStack book(int pages, boolean writable) {
      StringBuilder list = new StringBuilder();
      for (int i = 1; i <= pages; i++) list.append(i > 1 ? "," : "").append("\"page ").append(i).append('"');
      String snbt = writable
         ? "{id:\"minecraft:writable_book\",count:1,components:{\"minecraft:writable_book_content\":{pages:[" + list + "]}}}"
         : "{id:\"minecraft:written_book\",count:1,components:{\"minecraft:written_book_content\":{title:\"T\",author:\"A\",resolved:1b,pages:[" + list + "]}}}";
      try {
         return ItemStack.CODEC.parse(ops, net.minecraft.nbt.TagParser.parseCompoundFully(snbt)).getOrThrow();
      } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
         throw new IllegalArgumentException(snbt, e);
      }
   }

   static void maybuild(boolean on) {
      op("MAYBUILD " + (on ? 1 : 0));
      p.getAbilities().mayBuild = on;
      dump();
   }

   static Optional<Holder<net.minecraft.world.effect.MobEffect>> effect(String id) {
      if (id.equals("-")) return Optional.empty();
      return Optional.of(level.registryAccess().lookupOrThrow(Registries.MOB_EFFECT).getOrThrow(ResourceKey.create(Registries.MOB_EFFECT, Identifier.withDefaultNamespace(id))));
   }

   /** handleContainerSlotStateChanged for the open crafter menu (an in-sync client): toggles the slot of the crafter and broadcasts. */
   static void slotState(int slot, boolean enabled) {
      op("SLOTSTATE " + slot + " " + (enabled ? 1 : 0));
      if (!p.isSpectator() && menu instanceof CrafterMenu crafter && crafter.getContainer() instanceof net.minecraft.world.level.block.entity.CrafterBlockEntity be) {
         be.setSlotState(slot, enabled);
      }
      menu.broadcastChanges();
      dump();
   }

   /** ServerGamePacketListenerImpl.handleSetBeaconPacket: `R false` = the listener would disconnect, `R npe` = validateEffects throws (the packet processor suppresses it). */
   static void setBeacon(String primary, String secondary) {
      op("SETBEACON " + primary + " " + secondary);
      if (menu instanceof BeaconMenu beacon) {
         if (!beacon.stillValid(p)) {
            out("R skipped");
         } else {
            String r;
            try { r = String.valueOf(beacon.updateEffects(effect(primary), effect(secondary))); } catch (NullPointerException e) { r = "npe"; }
            menu.broadcastChanges();
            out("R " + r);
         }
      }
      dump();
   }

   static void events(boolean on) {
      op("EVENTS " + (on ? "on" : "off"));
      events = on;
      dump();
   }

   /** A random session on a fresh `k` menu: `count` random SET/GIVE/CLICK/DRAG/BUTTON ops over the stack specs of `pool` (`ticks`: see the class comment). */
   static void generate(String k, long seed, int count, String[] pool, boolean ticks) throws Exception {
      Random r = new Random(seed);
      OUT.println("# GEN " + k + " " + seed + " " + count);
      open(k, r.nextInt(31), r.nextInt(), r.nextInt(5) == 0, r.nextInt(1 << 24), r.nextInt(1 << 24), r.nextInt(16), 0);
      randomOps(r, count, pool, ticks);
      close();
   }

   /** `count` random ops on the menu that is open now (no OPEN, no CLOSE): the entity kinds open theirs with SPAWN and USE. */
   static void generateContinue(long seed, int count, String[] pool) {
      OUT.println("# GENCONT " + seed + " " + count);
      randomOps(new Random(seed), count, pool, false);
   }

   static void randomOps(Random r, int count, String[] pool, boolean ticks) {
      ContainerInput[] inputs = {ContainerInput.PICKUP, ContainerInput.PICKUP, ContainerInput.PICKUP, ContainerInput.QUICK_MOVE, ContainerInput.SWAP, ContainerInput.THROW,
         ContainerInput.CLONE, ContainerInput.PICKUP_ALL};
      for (int i = 0; i < count; i++) {
         int size = menu.slots.size();
         switch (r.nextInt(14)) {
            case 0, 1, 2 -> set(r.nextInt(size), mk(pool[r.nextInt(pool.length)]));
            case 3, 4 -> give(r.nextInt(41), mk(pool[r.nextInt(pool.length)]));
            case 5, 6, 7, 8, 9, 10, 11 -> {
               boolean outside = r.nextInt(12) == 0 && !ticks; // only a pickup leaves the window (the other inputs index a slot)
               ContainerInput in = outside ? ContainerInput.PICKUP : inputs[r.nextInt(inputs.length)];
               if (ticks && in == ContainerInput.THROW) in = ContainerInput.PICKUP; // the real player would pick the drops up while ticking
               int slot = outside ? -999 : r.nextInt(size);
               int button = switch (in) {
                  case SWAP -> r.nextInt(10) == 0 ? 40 : r.nextInt(9);
                  case PICKUP_ALL, CLONE -> 0;
                  default -> r.nextInt(2);
               };
               click(slot, button, in);
            }
            case 12 -> {
               int type = r.nextInt(p.getAbilities().instabuild ? 3 : 2);
               int[] slots = r.ints(1 + r.nextInt(4), 0, size).distinct().toArray();
               // the slots of a drag are placed in the order of an identity hash set: a crafting grid recomputes its result per slot, so keep one grid slot
               if ("crafting".equals(kind)) {
                  boolean[] seen = {false};
                  slots = java.util.stream.IntStream.of(slots).filter(s -> {
                     if (s < 1 || s > 9) return true;
                     boolean keep = !seen[0];
                     seen[0] = true;
                     return keep;
                  }).toArray();
               }
               drag(type, slots);
            }
            default -> {
               if (menu instanceof MerchantMenu) trade(r.nextInt(9) - 1);
               else if (ticks && r.nextBoolean()) tick(1 + r.nextInt(30));
               else if (menu instanceof CrafterMenu) slotState(r.nextInt(9), r.nextBoolean());
               else button(r.nextInt(4));
            }
         }
      }
   }

   /** Runs script lines (see the class comment). */
   static void script(List<String> lines) throws Exception {
      for (String raw : lines) {
         String line = raw.strip();
         if (line.isEmpty()) continue;
         if (line.startsWith("#")) { OUT.println(line); continue; }
         String[] a = line.split(" ");
         String rest = line.contains(" ") ? line.substring(line.indexOf(' ') + 1) : "";
         switch (a[0]) {
            case "OPEN" -> {
               long[] val = {0, 0, 0, 1, 1, 0, 0};
               for (int i = 2; i < a.length; i++) val[i - 2] = Long.parseLong(a[i]);
               open(a[1], (int) val[0], (int) val[1], val[2] != 0, val[3], val[4], (int) val[5], (int) val[6]);
            }
            case "SET" -> set(Integer.parseInt(a[1]), mk(a[2]));
            case "GIVE" -> give(Integer.parseInt(a[1]), mk(a[2]));
            case "CLICK" -> click(Integer.parseInt(a[1]), Integer.parseInt(a[2]), ContainerInput.valueOf(a[3]));
            case "DRAG" -> drag(Integer.parseInt(a[1]), Arrays.stream(a).skip(2).mapToInt(Integer::parseInt).toArray());
            case "BUTTON" -> button(Integer.parseInt(a[1]));
            case "NAME" -> name(rest);
            case "CLOSE" -> close();
            case "BLOCK" -> block(Integer.parseInt(a[1]), Integer.parseInt(a[2]), Integer.parseInt(a[3]), a[4]);
            case "TICK" -> tick(Integer.parseInt(a[1]));
            case "RECIPE" -> recipe(a[1], a[2]);
            case "GAMERULE" -> gamerule(a[1], a[2]);
            case "SEED" -> seed(a[1], Long.parseLong(a[2]));
            case "MAP" -> map(Integer.parseInt(a[1]), Integer.parseInt(a[2]), Integer.parseInt(a[3]), Integer.parseInt(a[4]), a[5].equals("1"));
            case "EVENTS" -> events(a[1].equals("on"));
            case "MAYBUILD" -> maybuild(a[1].equals("1"));
            case "SETBEACON" -> setBeacon(a[1], a[2]);
            case "SLOTSTATE" -> slotState(Integer.parseInt(a[1]), a[2].equals("1"));
            case "GEN" -> generate(a[1], Long.parseLong(a[2]), Integer.parseInt(a[3]), a[4].split("\\|"), a.length > 5 && a[5].equals("ticks"));
            case "GENCONT" -> generateContinue(Long.parseLong(a[1]), Integer.parseInt(a[2]), a[3].split("\\|"));
            case "SPAWN" -> spawn(a[1], a[2], Double.parseDouble(a[3]), Double.parseDouble(a[4]), Double.parseDouble(a[5]), Long.parseLong(a[6]), a.length > 7 ? a[7] : null);
            case "USE" -> use(a[1], a.length > 2 && a[2].equals("1"));
            case "TRADE" -> trade(Integer.parseInt(a[1]));
            case "NBT" -> nbt(a[1], a[2]);
            case "DISCARD" -> discard(a[1]);
            case "PLAYER" -> player(Double.parseDouble(a[1]), Double.parseDouble(a[2]), Double.parseDouble(a[3]));
            case "HEALTH" -> health(a[1]);
            case "MAPOFFERS" -> mapOffers(a[1]);
            default -> throw new IllegalArgumentException("unknown script line: " + line);
         }
      }
   }

   /** Boots the void world and joins the player next to TABLE (in reach of every menu block). */
   static void boot() throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "void";
      v = VanillaLevel.boot(o);
      registerKinds();
      level = v.level;
      fp = v.connect("Steve", GameType.SURVIVAL, 0.5, 71, 0.5, 0, 0, false);
      p = fp.player;
      menu = p.containerMenu;
      ops = level.registryAccess().createSerializationContext(NbtOps.INSTANCE);
      fp.drain();
      VanillaLevel.EVENTS.clear();
   }

   public static void main(String[] args) throws Exception {
      try {
         boot();
         script(Files.readAllLines(Path.of(args[0])));
      } catch (Throwable t) {
         // the bootstrap replaces System.err: the default handler would print nothing
         OUT.flush();
         t.printStackTrace(VanillaLevel.ERR);
         System.exit(1);
      }
      OUT.flush();
      System.exit(0);
   }
}
