// Golden generator for crates/mc-game/tests/golden/container_menu_golden.rs: scripted clicks through the REAL vanilla ChestMenu
// (rows 1..6, single + double chest), HopperMenu, DispenserMenu and ShulkerBoxMenu over SimpleContainers, and ChestMenu/DispenserMenu
// over real chest, barrel and dropper block entities and the player's ender chest. The scripts include bad slots and buttons (an
// exception of the click is an `EXC` event), malformed drags and items with a custom name, a max stack size or enchantments.
// Usage: scripts/golden-menu.sh
// Output: crates/mc-game/tests/fixtures/container_menu_golden.txt.gz
// Format: "OP ..." lines (the script, replayed by Rust) each followed by "OUT ..." lines (state after the op).
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSetPlayerInventoryPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.util.Prediction;
import net.minecraft.world.entity.*;
import net.minecraft.world.ItemStackWithSlot;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
import net.minecraft.world.Container;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.CompoundContainer;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import com.mojang.serialization.Codec;
import java.util.concurrent.Executor;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.ReloadableServerRegistries;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;

public class ContainerMenuGolden {
   static final StringBuilder OUT = new StringBuilder();
   static final List<String> events = new ArrayList<>();

   static void op(String s) { OUT.append("OP ").append(s).append('\n'); }
   static void out(String s) { OUT.append("OUT ").append(s).append('\n'); }

   static String st(ItemStack s) {
      if (s.isEmpty()) return "-";
      String n = BuiltInRegistries.ITEM.getKey(s.getItem()).getPath();
      StringBuilder b = new StringBuilder(n + "x" + s.getCount() + (s.isDamaged() ? "d" + s.getDamageValue() : ""));
      net.minecraft.world.item.component.BundleContents bc = s.get(net.minecraft.core.component.DataComponents.BUNDLE_CONTENTS);
      if (bc != null) {
         b.append('{');
         for (net.minecraft.world.item.ItemStackTemplate t : bc.items()) b.append(BuiltInRegistries.ITEM.getKey(t.item().value()).getPath()).append('x').append(t.count()).append(',');
         b.append(';').append(bc.getSelectedItemIndex()).append('}');
      }
      return b.append(marks(s)).toString();
   }

   /** Components that make stacks differ besides item/count/damage: custom name, enchantments, a max stack size override. */
   static String marks(ItemStack s) {
      StringBuilder b = new StringBuilder();
      Component name = s.get(DataComponents.CUSTOM_NAME);
      if (name != null) b.append("+n(").append(name.getString()).append(')');
      net.minecraft.world.item.enchantment.ItemEnchantments ench = s.get(DataComponents.ENCHANTMENTS);
      if (ench != null && !ench.isEmpty()) {
         b.append("+e(");
         for (var e : ench.entrySet()) b.append(e.getKey().unwrapKey().orElseThrow().identifier().getPath()).append(':').append(e.getIntValue()).append(',');
         b.append(')');
      }
      int max = s.getMaxStackSize();
      if (max != new ItemStack(s.getItem()).getMaxStackSize()) b.append("+m(").append(max).append(')');
      return b.toString();
   }

   static boolean creative = false;

   public static class RecListener extends ServerGamePacketListenerImpl {
      public RecListener() { super(null, null, null, null); }
      @Override public void send(Packet<?> p) {
         if (p instanceof ClientboundSetPlayerInventoryPacket x) events.add("PKT set_player_inventory " + x.slot() + " " + st(x.contents()));
         else events.add("PKT other");
      }
   }

   public static class FakeLevel extends ServerLevel {
      public FakeLevel() { super(null, null, null, null, null, null, false, 0L, null, false); }
      public net.minecraft.util.RandomSource rnd;
      @Override public net.minecraft.util.RandomSource getRandom() { return rnd; }
      /** Set by `Level.blockEntityChanged`, which every `BlockEntity.setChanged` calls (the chunk is marked unsaved there). */
      @Override public void blockEntityChanged(BlockPos pos) { dirty = true; }
      @Override public void updateNeighbourForOutputSignal(BlockPos pos, net.minecraft.world.level.block.Block block) { }
      @Override public net.minecraft.world.flag.FeatureFlagSet enabledFeatures() { return FeatureFlags.DEFAULT_FLAGS; }
   }

   public static class FakePlayer extends ServerPlayer {
      public FakePlayer() { super(null, null, null, null); }
      @Override public boolean isSpectator() { return false; }
      @Override public boolean isCreative() { return creative; }
      @Override public boolean hasDisconnected() { return false; }
      @Override public ItemEntity drop(ItemStack s, boolean thrown, Prediction p) { events.add("DROP " + st(s) + " " + thrown + " " + p); return null; }
      @Override public void onEquipItem(EquipmentSlot slot, ItemStack o, ItemStack n) { events.add("EQUIP " + slot.getName() + " " + st(o) + " " + st(n)); }
      @Override public void playSound(net.minecraft.sounds.SoundEvent sound, float volume, float pitch) {
         events.add("SOUND " + BuiltInRegistries.SOUND_EVENT.getKey(sound).getPath() + " " + Float.floatToIntBits(volume) + " " + Float.floatToIntBits(pitch));
      }
      public ServerLevel lvl;
      @Override public ServerLevel level() { return lvl; }
   }

   static sun.misc.Unsafe U;
   static void set(Class<?> c, Object o, String f, Object v) throws Exception {
      Field fld = c.getDeclaredField(f); fld.setAccessible(true); fld.set(o, v);
   }

   static final String[] PALETTE = {"stone", "dirt", "cobblestone", "diamond_pickaxe", "ender_pearl", "snowball", "diamond_helmet",
      "diamond_chestplate", "diamond_leggings", "diamond_boots", "shield", "apple", "bucket", "elytra", "carved_pumpkin", "iron_helmet",
      "bundle", "bundle", "red_bundle", "bundle", "stone", "dirt"};

   static HolderLookup.Provider PROV;
   static Holder<Enchantment> ench(String path) {
      return PROV.lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(ResourceKey.create(Registries.ENCHANTMENT, Identifier.withDefaultNamespace(path)));
   }

   static ItemStack mk(String name, int count, int dmg) {
      ItemStack s = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(name)), count);
      if (dmg > 0) s.setDamageValue(dmg);
      return s;
   }

   /** `mods`: `-` or a comma list of `bind` (binding curse), `prot` (protection 3), `name:X`, `max:N` (max stack size override). */
   static ItemStack mk(String name, int count, int dmg, String mods) {
      ItemStack s = mk(name, count, dmg);
      if (!mods.equals("-")) for (String m : mods.split(",")) {
         if (m.equals("bind")) s.enchant(ench("binding_curse"), 1);
         else if (m.equals("prot")) s.enchant(ench("protection"), 3);
         else if (m.startsWith("name:")) s.set(DataComponents.CUSTOM_NAME, Component.literal(m.substring(5)));
         else if (m.startsWith("max:")) s.set(DataComponents.MAX_STACK_SIZE, Integer.parseInt(m.substring(4)));
         else throw new IllegalStateException(m);
      }
      return s;
   }

   /** At most one enchantment per stack (the order of several is not specified); no max override on damageable items (vanilla rejects the pair). */
   static String pickMods(Random r, String name) {
      boolean damageable = mk(name, 1, 0).isDamageableItem();
      int[] maxes = {1, 2, 3, 5, 99};
      switch (r.nextInt(14)) {
         case 0: return "bind";
         case 1: return "prot";
         case 2: return "name:Foo";
         case 3: return "name:Bar,bind";
         case 4: return damageable ? "-" : "max:" + maxes[r.nextInt(maxes.length)];
         case 5: return damageable ? "name:Foo" : "name:Foo,max:3";
         default: return "-";
      }
   }

   static final int[] BAD_BUTTONS = {2, 3, 5, 8, 40, -1, 100};
   static final int[] SWAP_BAD = {9, 10, 41, -1, 100};
   static int badSlot(Random r, int n) { int[] v = {-1, -2, -999, n, n + 1, 100}; return v[r.nextInt(v.length)]; }

   /** True while the scenario's container is a block entity: `DIRTY` lines then say whether `setChanged` ran since the last dump. */
   static boolean beBacked;
   static boolean dirty;

   static StringBuilder sb;
   static void dump(AbstractContainerMenu menu, Player p, List<String> sync) {
      StringBuilder b = new StringBuilder("C " + st(menu.getCarried()));
      out(b.toString());
      b = new StringBuilder("S");
      for (int i = 0; i < menu.slots.size(); i++) { ItemStack s = menu.slots.get(i).getItem(); if (!s.isEmpty()) b.append(' ').append(i).append(':').append(st(s)); }
      out(b.toString());
      b = new StringBuilder("I");
      Inventory inv = p.getInventory();
      for (int i = 0; i < inv.getContainerSize(); i++) { ItemStack s = inv.getItem(i); if (!s.isEmpty()) b.append(' ').append(i).append(':').append(st(s)); }
      out(b.toString());
      out("SEL " + inv.getSelectedSlot());
      if (beBacked) { out("DIRTY " + (dirty ? 1 : 0)); dirty = false; }
      for (String e : events) out(e);
      events.clear();
      for (String e : sync) out(e);
      sync.clear();
   }

   public static void main(String[] args) throws Exception {
      try { run(); } catch (Throwable t) { t.printStackTrace(); System.exit(1); }
   }

   static final String[] CPAL = {"stone", "dirt", "cobblestone", "diamond_pickaxe", "ender_pearl", "snowball", "apple", "bucket",
      "bundle", "shulker_box", "red_shulker_box", "iron_helmet", "stone", "dirt", "oak_planks", "diamond_chestplate", "shield"};

   /** Block entities with `startOpen`/`stopOpen` silenced (the opener counter needs a real level: sounds, game events, ticks). */
   static Container chest(Level lvl, int x) {
      ChestBlockEntity be = new ChestBlockEntity(new BlockPos(x, 64, 0), Blocks.CHEST.defaultBlockState()) {
         @Override public void startOpen(ContainerUser u) { }
         @Override public void stopOpen(ContainerUser u) { }
      };
      be.setLevel(lvl);
      return be;
   }

   static final int KINDS = 14;

   static AbstractContainerMenu build(int kind, int id, Inventory inv, Player p, Level lvl, Container[] holder, int[] size) {
      beBacked = kind == 9 || kind == 10 || kind == 12 || kind == 13;
      switch (kind) {
         case 0: { Container c = new SimpleContainer(27); holder[0] = c; size[0] = 27; return new ChestMenu(MenuType.GENERIC_9x3, id, inv, c, 3); }
         case 1: { Container a = new SimpleContainer(27), b = new SimpleContainer(27); Container c = new CompoundContainer(a, b); holder[0] = c; size[0] = 54; return new ChestMenu(MenuType.GENERIC_9x6, id, inv, c, 6); }
         case 2: { Container c = new SimpleContainer(5); holder[0] = c; size[0] = 5; return new HopperMenu(id, inv, c); }
         case 3: { Container c = new SimpleContainer(9); holder[0] = c; size[0] = 9; return new DispenserMenu(id, inv, c); }
         case 4: { Container c = new SimpleContainer(27); holder[0] = c; size[0] = 27; return new ShulkerBoxMenu(id, inv, c); }
         case 5: { Container c = new SimpleContainer(9); holder[0] = c; size[0] = 9; return new ChestMenu(MenuType.GENERIC_9x1, id, inv, c, 1); }
         case 6: { Container c = new SimpleContainer(18); holder[0] = c; size[0] = 18; return new ChestMenu(MenuType.GENERIC_9x2, id, inv, c, 2); }
         case 7: { Container c = new SimpleContainer(36); holder[0] = c; size[0] = 36; return new ChestMenu(MenuType.GENERIC_9x4, id, inv, c, 4); }
         case 8: { Container c = new SimpleContainer(45); holder[0] = c; size[0] = 45; return new ChestMenu(MenuType.GENERIC_9x5, id, inv, c, 5); }
         case 9: { Container c = chest(lvl, 0); holder[0] = c; size[0] = 27; return new ChestMenu(MenuType.GENERIC_9x3, id, inv, c, 3); }
         case 10: {
            BarrelBlockEntity be = new BarrelBlockEntity(new BlockPos(0, 64, 0), Blocks.BARREL.defaultBlockState()) {
               @Override public void startOpen(ContainerUser u) { }
               @Override public void stopOpen(ContainerUser u) { }
            };
            be.setLevel(lvl);
            holder[0] = be; size[0] = 27; return new ChestMenu(MenuType.GENERIC_9x3, id, inv, be, 3);
         }
         case 11: { Container c = p.getEnderChestInventory(); holder[0] = c; size[0] = 27; return new ChestMenu(MenuType.GENERIC_9x3, id, inv, c, 3); }
         case 12: {
            DropperBlockEntity be = new DropperBlockEntity(new BlockPos(0, 64, 0), Blocks.DROPPER.defaultBlockState());
            be.setLevel(lvl);
            holder[0] = be; size[0] = 9; return new DispenserMenu(id, inv, be);
         }
         default: { Container c = new CompoundContainer(chest(lvl, 0), chest(lvl, 1)); holder[0] = c; size[0] = 54; return new ChestMenu(MenuType.GENERIC_9x6, id, inv, c, 6); }
      }
   }

   /** A random stack in a random slot of the container (`cont` op) or the player's inventory (`inv` op). */
   static void fill(Random r, Inventory inv, Container container, int size) {
      boolean inContainer = r.nextBoolean();
      String name = CPAL[r.nextInt(CPAL.length)];
      String mods = pickMods(r, name);
      ItemStack probe = mk(name, 1, 0, mods);
      int count = 1 + r.nextInt(Math.max(1, probe.getMaxStackSize()));
      int dmg = probe.isDamageableItem() && r.nextInt(3) == 0 ? 1 + r.nextInt(5) : 0;
      if (inContainer) {
         int slot = r.nextInt(size);
         op("cont " + slot + " " + name + " " + count + " " + dmg + " " + mods);
         container.setItem(slot, mk(name, count, dmg, mods));
      } else {
         int slot = r.nextInt(37);
         if (slot == 36) slot = 40; // the offhand is a SWAP target
         op("inv " + slot + " " + name + " " + count + " " + dmg + " " + mods);
         inv.setItem(slot, mk(name, count, dmg, mods));
      }
   }

   /** One scripted click; an exception of the click (a slot index outside the menu) is the `EXC` event, the menu state stays as it was left. */
   static void click(AbstractContainerMenu menu, Player p, int slot, int button, ContainerInput in) {
      op("click " + slot + " " + button + " " + in.name());
      try { menu.clicked(slot, button, in, p); } catch (Exception e) { events.add("EXC"); }
   }

   static void clickDump(AbstractContainerMenu menu, Player p, List<String> sync, int slot, int button, ContainerInput in) {
      click(menu, p, slot, button, in);
      menu.broadcastChanges();
      dump(menu, p, sync);
   }

   static void run() throws Exception {
      extractor.Setup.bootstrap();
      Executor direct = Runnable::run;
      MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources()));
      LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
      List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
      staticTags.forEach(Registry.PendingTags::apply);
      var worldContext = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
      var world = RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, direct).join();
      List<HolderLookup.RegistryLookup<?>> dimContext = new ArrayList<>(worldContext);
      world.listRegistries().forEach(dimContext::add);
      var dims = RegistryDataLoader.load(resources, dimContext, RegistryDataLoader.DIMENSION_REGISTRIES, direct).join();
      layers = layers.replaceFrom(RegistryLayer.WORLD, world, dims);
      var loaded = ReloadableServerRegistries.reload(layers, staticTags, resources, direct).join();
      var comps = BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(loaded.lookupWithUpdatedTags());
      comps.forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
      PROV = loaded.lookupWithUpdatedTags();
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); uf.setAccessible(true); U = (sun.misc.Unsafe) uf.get(null);
      int scenario = 0;
      for (int kind = 0; kind < KINDS; kind++) for (int cr = 0; cr < 2; cr++, scenario++) {
         creative = cr == 1;
         Random r = new Random(5000 + scenario);
         FakePlayer p = (FakePlayer) U.allocateInstance(FakePlayer.class);
         ServerLevel lvl = (ServerLevel) U.allocateInstance(FakeLevel.class);
         ((FakeLevel) lvl).rnd = net.minecraft.util.RandomSource.create(42L + scenario);
         p.lvl = lvl;
         set(Entity.class, p, "type", EntityTypes.PLAYER);
         EntityEquipment eq = new PlayerEquipment(p);
         Inventory inv = new Inventory(p, eq);
         set(Player.class, p, "inventory", inv);
         set(LivingEntity.class, p, "equipment", eq);
         set(Player.class, p, "enderChestInventory", new PlayerEnderChestContainer());
         Abilities ab = new Abilities(); ab.instabuild = creative;
         set(Player.class, p, "abilities", ab);
         p.connection = (RecListener) U.allocateInstance(RecListener.class);
         final List<String> sync = new ArrayList<>();
         Container[] holder = new Container[1]; int[] size = new int[1];
         AbstractContainerMenu menu = build(kind, 3, inv, p, lvl, holder, size);
         set(Player.class, p, "containerMenu", menu);
         op("scenario " + scenario + " " + kind + " creative=" + creative);
         for (int k = 0; k < 24; k++) fill(r, inv, holder[0], size[0]);
         dirty = false;
         menu.setSynchronizer(new ContainerSynchronizer() {
            public void sendInitialData(AbstractContainerMenu c, List<ItemStack> items, ItemStack carried, int[] data) {
               StringBuilder b = new StringBuilder("SYNC init " + c.incrementStateId() + " " + st(carried));
               for (int i = 0; i < items.size(); i++) if (!items.get(i).isEmpty()) b.append(' ').append(i).append(':').append(st(items.get(i)));
               sync.add(b.toString());
            }
            public void sendSlotChange(AbstractContainerMenu c, int i, ItemStack s) { sync.add("SYNC slot " + c.incrementStateId() + " " + i + " " + st(s)); }
            public void sendCarriedChange(AbstractContainerMenu c, ItemStack s) { sync.add("SYNC carried " + st(s)); }
            public void sendDataChange(AbstractContainerMenu c, int id, int v) { sync.add("SYNC data " + id + " " + v); }
            public RemoteSlot createSlot() { return new RemoteSlot.Synchronized(comp -> 0); }
         });
         op("sync");
         dump(menu, p, sync);
         int n = menu.slots.size();
         for (int step = 0; step < 240; step++) {
            int kindOp = r.nextInt(100);
            int slot = r.nextInt(n);
            if (r.nextInt(30) == 0) slot = badSlot(r, n);
            int button = r.nextInt(12) == 0 ? BAD_BUTTONS[r.nextInt(BAD_BUTTONS.length)] : r.nextInt(2);
            if (kindOp < 28) {
               if (r.nextInt(10) == 0) slot = -999;
               click(menu, p, slot, button, ContainerInput.PICKUP);
            } else if (kindOp < 46) {
               click(menu, p, slot, button, ContainerInput.QUICK_MOVE);
            } else if (kindOp < 54) {
               int b = r.nextInt(10); if (b == 9) b = 40;
               if (r.nextInt(8) == 0) b = SWAP_BAD[r.nextInt(SWAP_BAD.length)];
               click(menu, p, slot, b, ContainerInput.SWAP);
            } else if (kindOp < 58) {
               click(menu, p, slot, button, ContainerInput.CLONE);
            } else if (kindOp < 65) {
               click(menu, p, slot, button, ContainerInput.THROW);
            } else if (kindOp < 73) {
               click(menu, p, slot, button, ContainerInput.PICKUP_ALL);
            } else if (kindOp < 83) {
               int type = r.nextInt(3);
               click(menu, p, -999, AbstractContainerMenu.getQuickcraftMask(0, type), ContainerInput.QUICK_CRAFT);
               menu.broadcastChanges(); dump(menu, p, sync);
               int cnt = 1 + r.nextInt(5);
               for (int i = 0; i < cnt; i++) {
                  int s2 = r.nextInt(n);
                  click(menu, p, s2, AbstractContainerMenu.getQuickcraftMask(1, type), ContainerInput.QUICK_CRAFT);
                  menu.broadcastChanges(); dump(menu, p, sync);
               }
               click(menu, p, -999, AbstractContainerMenu.getQuickcraftMask(2, type), ContainerInput.QUICK_CRAFT);
            } else if (kindOp < 92) {
               // malformed and interleaved drags: mostly in order, sometimes a wrong header (3, or out of order), type 3, a raw button, a bad slot, another click in between
               int len = 1 + r.nextInt(8), prev = -1;
               ContainerInput[] others = {ContainerInput.PICKUP, ContainerInput.QUICK_MOVE, ContainerInput.SWAP, ContainerInput.CLONE, ContainerInput.THROW, ContainerInput.PICKUP_ALL};
               for (int i = 0; i < len; i++) {
                  int header = r.nextInt(4) == 0 ? r.nextInt(4) : prev < 0 || prev == 2 ? 0 : prev == 0 ? 1 : (r.nextBoolean() ? 1 : 2);
                  int type = r.nextInt(6) == 0 ? 3 : r.nextInt(3);
                  int mask = r.nextInt(12) == 0 ? r.nextInt(80) - 8 : AbstractContainerMenu.getQuickcraftMask(header, type);
                  prev = AbstractContainerMenu.getQuickcraftHeader(mask);
                  int s2 = prev == 1 || r.nextInt(5) == 0 ? (r.nextInt(8) == 0 ? badSlot(r, n) : r.nextInt(n)) : -999;
                  if (r.nextInt(7) == 0) clickDump(menu, p, sync, r.nextInt(n), r.nextInt(2), others[r.nextInt(others.length)]);
                  if (i == len - 1) click(menu, p, s2, mask, ContainerInput.QUICK_CRAFT);
                  else clickDump(menu, p, sync, s2, mask, ContainerInput.QUICK_CRAFT);
               }
            } else if (kindOp < 96) {
               // CLONE with a carried stack (ignored), then once the carried stack is gone (clones in creative)
               int from = r.nextInt(n);
               for (int i = 0; i < n; i++) if (!menu.slots.get((from + i) % n).getItem().isEmpty()) { clickDump(menu, p, sync, (from + i) % n, r.nextInt(2), ContainerInput.PICKUP); break; }
               clickDump(menu, p, sync, r.nextInt(n), 0, ContainerInput.CLONE);
               clickDump(menu, p, sync, -999, 0, ContainerInput.PICKUP);
               click(menu, p, r.nextInt(n), 0, ContainerInput.CLONE);
            } else {
               fill(r, inv, holder[0], size[0]);
            }
            menu.broadcastChanges();
            dump(menu, p, sync);
         }
         op("remove");
         menu.removed(p);
         menu.broadcastChanges();
         dump(menu, p, sync);
         StringBuilder b = new StringBuilder("CONT");
         for (int i = 0; i < size[0]; i++) { ItemStack s = holder[0].getItem(i); if (!s.isEmpty()) b.append(' ').append(i).append(':').append(st(s)); }
         out(b.toString());
      }
      System.out.print(OUT);
   }
}
