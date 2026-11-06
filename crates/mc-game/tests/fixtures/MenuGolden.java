// Golden generator for crates/mc-game/tests/golden/menu_golden.rs: scripted container clicks through the REAL vanilla
// InventoryMenu / Inventory / AbstractContainerMenu with a constructor-less ServerPlayer subclass.
// Usage: scripts/golden-menu.sh
// Output: crates/mc-game/tests/fixtures/menu_golden.txt.gz
// Format: "OP ..." lines (the script, replayed by Rust) each followed by "OUT ..." lines (state after the op).
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.NonNullList;
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
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import com.mojang.serialization.Codec;

public class MenuGolden {
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

   static ItemStack mk(String name, int count, int dmg) {
      ItemStack s = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(name)), count);
      if (dmg > 0) s.setDamageValue(dmg);
      return s;
   }

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
      for (String e : events) out(e);
      events.clear();
      for (String e : sync) out(e);
      sync.clear();
   }

   public static void main(String[] args) throws Exception {
      try { run(); } catch (Throwable t) { t.printStackTrace(); System.exit(1); }
   }

   static void run() throws Exception {
      extractor.Setup.bootstrap();
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); uf.setAccessible(true); U = (sun.misc.Unsafe) uf.get(null);
      long seed = 1;
      for (int scenario = 0; scenario < 8; scenario++) {
         creative = scenario % 2 == 1;
         Random r = new Random(1000 + scenario);
         FakePlayer p = (FakePlayer) U.allocateInstance(FakePlayer.class);
         ServerLevel lvl = (ServerLevel) U.allocateInstance(FakeLevel.class);
         ((FakeLevel) lvl).rnd = net.minecraft.util.RandomSource.create(42L + scenario);
         p.lvl = lvl;
         set(Entity.class, p, "type", EntityTypes.PLAYER);
         EntityEquipment eq = new PlayerEquipment(p);
         Inventory inv = new Inventory(p, eq);
         set(Player.class, p, "inventory", inv);
         set(LivingEntity.class, p, "equipment", eq);
         Abilities ab = new Abilities(); ab.instabuild = creative;
         set(Player.class, p, "abilities", ab);
         p.connection = (RecListener) U.allocateInstance(RecListener.class);
         final List<String> sync = new ArrayList<>();
         InventoryMenu menu = new InventoryMenu(inv, true, p) {
            // `CraftingMenu.slotChangedCraftingGrid` needs a server/recipe manager: emulate it with zero recipes
            @Override public void slotsChanged(Container c) {
               resultSlots.setItem(0, ItemStack.EMPTY);
               setRemoteSlot(0, ItemStack.EMPTY);
               sync.add("SYNC slot " + incrementStateId() + " 0 -");
            }
         };
         set(Player.class, p, "inventoryMenu", menu);
         set(Player.class, p, "containerMenu", menu);
         op("scenario " + scenario + " creative=" + creative);
         // initial contents: 12 random stacks in the main inventory/armor/offhand slots
         for (int k = 0; k < 14; k++) {
            int slot = r.nextInt(43);
            String name = PALETTE[r.nextInt(PALETTE.length)];
            ItemStack probe = mk(name, 1, 0);
            int max = probe.getMaxStackSize();
            int count = 1 + r.nextInt(Math.max(1, max));
            int dmg = probe.isDamageableItem() && r.nextInt(3) == 0 ? 1 + r.nextInt(5) : 0;
            ItemStack s = mk(name, count, dmg);
            op("inv " + slot + " " + name + " " + count + " " + dmg);
            inv.setItem(slot, s);
         }
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

         for (int step = 0; step < 220; step++) {
            int kind = r.nextInt(100);
            int slot = 5 + r.nextInt(41); // 5..45 (grid/result need the recipe manager)
            int button = r.nextInt(2);
            if (kind < 30) {
               if (r.nextInt(10) == 0) slot = -999;
               op("click " + slot + " " + button + " PICKUP");
               menu.clicked(slot, button, ContainerInput.PICKUP, p);
            } else if (kind < 45) {
               op("click " + slot + " " + button + " QUICK_MOVE");
               menu.clicked(slot, button, ContainerInput.QUICK_MOVE, p);
            } else if (kind < 55) {
               int b = r.nextInt(10); if (b == 9) b = 40;
               op("click " + slot + " " + b + " SWAP");
               menu.clicked(slot, b, ContainerInput.SWAP, p);
            } else if (kind < 60) {
               op("click " + slot + " 0 CLONE");
               menu.clicked(slot, 0, ContainerInput.CLONE, p);
            } else if (kind < 68) {
               op("click " + slot + " " + button + " THROW");
               menu.clicked(slot, button, ContainerInput.THROW, p);
            } else if (kind < 76) {
               op("click " + slot + " " + button + " PICKUP_ALL");
               menu.clicked(slot, button, ContainerInput.PICKUP_ALL, p);
            } else if (kind < 92) {
               int type = r.nextInt(3);
               op("click -999 " + AbstractContainerMenu.getQuickcraftMask(0, type) + " QUICK_CRAFT");
               menu.clicked(-999, AbstractContainerMenu.getQuickcraftMask(0, type), ContainerInput.QUICK_CRAFT, p);
               menu.broadcastChanges(); dump(menu, p, sync);
               int n = 1 + r.nextInt(5);
               for (int i = 0; i < n; i++) {
                  int s2 = 5 + r.nextInt(41);
                  op("click " + s2 + " " + AbstractContainerMenu.getQuickcraftMask(1, type) + " QUICK_CRAFT");
                  menu.clicked(s2, AbstractContainerMenu.getQuickcraftMask(1, type), ContainerInput.QUICK_CRAFT, p);
                  menu.broadcastChanges(); dump(menu, p, sync);
               }
               op("click -999 " + AbstractContainerMenu.getQuickcraftMask(2, type) + " QUICK_CRAFT");
               menu.clicked(-999, AbstractContainerMenu.getQuickcraftMask(2, type), ContainerInput.QUICK_CRAFT, p);
            } else if (kind < 96) {
               String name = PALETTE[r.nextInt(PALETTE.length)];
               ItemStack probe = mk(name, 1, 0);
               int count = 1 + r.nextInt(Math.max(1, probe.getMaxStackSize()));
               op("add " + name + " " + count);
               ItemStack s = mk(name, count, 0);
               boolean ok = inv.add(s);
               events.add("ADD " + ok + " " + st(s));
            } else if (kind < 98) {
               int sel = r.nextInt(9);
               op("select " + sel);
               inv.setSelectedSlot(sel);
            } else if (kind < 99) {
               int bs = 5 + r.nextInt(41);
               int bi = r.nextInt(5) - 1;
               op("bundlesel " + bs + " " + bi);
               menu.setSelectedBundleItemIndex(bs, bi);
            } else {
               // creative-mode slot set (handleSetCreativeModeSlot): only meaningful for infinite materials
               String name = PALETTE[r.nextInt(PALETTE.length)];
               ItemStack probe = mk(name, 1, 0);
               int count = 1 + r.nextInt(probe.getMaxStackSize());
               int cs = 5 + r.nextInt(41);
               op("creative " + cs + " " + name + " " + count);
               if (p.hasInfiniteMaterials()) {
                  ItemStack s = mk(name, count, 0);
                  menu.getSlot(cs).setByPlayer(s);
                  menu.setRemoteSlot(cs, s);
               }
            }
            menu.broadcastChanges();
            dump(menu, p, sync);
         }
         // close: removed() (carried item placed back / dropped), then final state
         op("remove");
         menu.removed(p);
         menu.broadcastChanges();
         dump(menu, p, sync);

         // NBT round trip of the main inventory (ItemStackWithSlot list)
         List<String> tags = new ArrayList<>();
         ListTag list = new ListTag();
         for (int i = 0; i < 36; i++) {
            ItemStack s = inv.getItem(i);
            if (!s.isEmpty()) list.add(ItemStackWithSlot.CODEC.encodeStart(NbtOps.INSTANCE, new ItemStackWithSlot(i, s)).getOrThrow());
         }
         out("NBT " + list);
         Inventory inv2 = new Inventory(p, new PlayerEquipment(p));
         NonNullList<ItemStack> dst = inv2.getNonEquipmentItems();
         for (Tag t : list) {
            ItemStackWithSlot w = ItemStackWithSlot.CODEC.parse(NbtOps.INSTANCE, t).getOrThrow();
            if (w.isValidInContainer(dst.size())) dst.set(w.slot(), w.stack());
         }
         StringBuilder b = new StringBuilder("NBTLOAD");
         for (int i = 0; i < 36; i++) if (!inv2.getItem(i).isEmpty()) b.append(' ').append(i).append(':').append(st(inv2.getItem(i)));
         out(b.toString());
      }
      System.out.print(OUT);
   }
}
