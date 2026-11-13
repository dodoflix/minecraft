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

import net.minecraft.SharedConstants;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.*;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.item.crafting.*;
import java.util.concurrent.Executor;
import net.minecraft.server.dedicated.DedicatedServer;
import net.minecraft.stats.ServerRecipeBook;
import net.minecraft.world.level.gamerules.GameRules;
// Golden generator for crates/mc-game/tests/golden/place_recipe_golden.rs: the REAL vanilla InventoryMenu.handlePlacement
// (ServerPlaceRecipe + StackedItemContents, with the grid recompute `slotsChanged` runs for every grid change and its
// result slot packets) over random inventories, using the real RecipeManager.
// Usage: scripts/golden-recipes.sh
// Output: crates/mc-game/tests/fixtures/place_recipe_golden.txt
// Format: "OP ..." lines (replayed by Rust) each followed by "OUT ..." lines (state after the op).
public class PlaceRecipeGolden {
   static final StringBuilder OUT = new StringBuilder();
   static final List<String> events = new ArrayList<>();
   static void op(String s) { OUT.append("OP ").append(s).append('\n'); }
   static void out(String s) { OUT.append("OUT ").append(s).append('\n'); }
   static String st(ItemStack s) {
      if (s.isEmpty()) return "-";
      return BuiltInRegistries.ITEM.getKey(s.getItem()).getPath() + "x" + s.getCount();
   }
   static boolean creative = false;

   /** The synchronizer lines of the running scenario (`CraftingMenu.slotChangedCraftingGrid` sends its slot packet past the synchronizer). */
   static List<String> syncLines;
   public static class RecListener extends ServerGamePacketListenerImpl {
      public RecListener() { super(null, null, null, null); }
      @Override public void send(Packet<?> p) {
         if (p instanceof net.minecraft.network.protocol.game.ClientboundContainerSetSlotPacket x) syncLines.add("SYNC slot " + x.getStateId() + " " + x.getSlot() + " " + st(x.getItem()));
         else events.add("PKT other");
      }
   }
   public static class FakeServer extends DedicatedServer {
      public FakeServer() { super(null, null, null, null, null, null, null, null, null, null); }
      public RecipeManager recipes;
      @Override public RecipeManager getRecipeManager() { return recipes; }
   }
   public static class FakeLevel extends ServerLevel {
      public FakeLevel() { super(null, null, null, null, null, null, false, 0L, null, false); }
      public FakeServer server;
      public GameRules rules;
      @Override public net.minecraft.world.flag.FeatureFlagSet enabledFeatures() { return FeatureFlags.DEFAULT_FLAGS; }
      @Override public MinecraftServer getServer() { return server; }
      @Override public RecipeManager recipeAccess() { return server.recipes; }
      @Override public GameRules getGameRules() { return rules; }
   }
   public static class FakePlayer extends ServerPlayer {
      public FakePlayer() { super(null, null, null, null); }
      @Override public boolean isSpectator() { return false; }
      @Override public boolean isCreative() { return creative; }
      @Override public boolean hasDisconnected() { return false; }
      @Override public ItemEntity drop(ItemStack s, boolean thrown, Prediction p) { events.add("DROP " + st(s) + " " + thrown + " " + p); return null; }
      public ServerLevel lvl;
      @Override public ServerLevel level() { return lvl; }
      public ServerRecipeBook book;
      @Override public ServerRecipeBook getRecipeBook() { return book; }
   }
   static sun.misc.Unsafe U;
   static void set(Class<?> c, Object o, String f, Object v) throws Exception {
      Field fld = c.getDeclaredField(f); fld.setAccessible(true); fld.set(o, v);
   }

   static final String[] PALETTE = {"oak_planks", "oak_log", "stick", "cobblestone", "iron_ingot", "gold_ingot", "diamond", "coal", "string", "paper", "dirt", "apple"};
   static final Set<Item> PALETTE_ITEMS = new HashSet<>();

   static void dump(InventoryMenu menu, Player p, String action) {
      out("action " + action);
      StringBuilder b = new StringBuilder("grid");
      for (int i = 1; i <= 4; i++) b.append(' ').append(st(menu.getSlot(i).getItem()));
      out(b.toString());
      out("result " + st(menu.getSlot(0).getItem()));
      b = new StringBuilder("inv");
      Inventory inv = p.getInventory();
      for (int i = 0; i < inv.getContainerSize(); i++) { ItemStack s = inv.getItem(i); if (!s.isEmpty()) b.append(' ').append(i).append(':').append(st(s)); }
      out(b.toString());
      for (String e : events) out(e);
      events.clear();
      for (String e : syncLines) out(e);
      syncLines.clear();
   }

   public static void main(String[] args) throws Exception {
      try { run(); } catch (Throwable t) { t.printStackTrace(); System.exit(1); }
      System.out.print(OUT);
   }

   static void run() throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
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
      RecipeManager rm = new RecipeManager(loaded.lookupWithUpdatedTags());
      rm.finalizeRecipeLoading(FeatureFlags.VANILLA_SET);
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); uf.setAccessible(true); U = (sun.misc.Unsafe) uf.get(null);
      for (String n : PALETTE) PALETTE_ITEMS.add(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(n)));

      // recipes of the 2x2 grid whose ingredients each accept exactly one palette item
      List<RecipeHolder<?>> candidates = new ArrayList<>();
      for (RecipeHolder<?> h : rm.getRecipes()) {
         Recipe<?> r = h.value();
         if (r.isSpecial() || r.getType() != RecipeType.CRAFTING) continue;
         boolean fits = r instanceof ShapedRecipe sr ? sr.getWidth() <= 2 && sr.getHeight() <= 2 : r instanceof ShapelessRecipe && r.placementInfo().ingredients().size() <= 4;
         if (!fits || r.placementInfo().isImpossibleToPlace()) continue;
         boolean ok = true;
         for (Ingredient i : r.placementInfo().ingredients()) {
            long n = i.items().filter(x -> PALETTE_ITEMS.contains(x.value())).count();
            if (n != 1) ok = false;
         }
         if (ok) candidates.add(h);
      }
      System.err.println("candidates " + candidates.size());

      for (int scenario = 0; scenario < 6; scenario++) {
         creative = scenario % 3 == 2;
         Random r = new Random(77 + scenario);
         FakePlayer p = (FakePlayer) U.allocateInstance(FakePlayer.class);
         FakeLevel lvl = (FakeLevel) U.allocateInstance(FakeLevel.class);
         FakeServer server = (FakeServer) U.allocateInstance(FakeServer.class);
         server.recipes = rm;
         lvl.server = server;
         lvl.rules = new GameRules(FeatureFlags.DEFAULT_FLAGS);
         p.lvl = lvl;
         p.book = new ServerRecipeBook((id, output) -> { });
         set(Entity.class, p, "type", EntityTypes.PLAYER);
         EntityEquipment eq = new PlayerEquipment(p);
         Inventory inv = new Inventory(p, eq);
         set(Player.class, p, "inventory", inv);
         set(LivingEntity.class, p, "equipment", eq);
         Abilities ab = new Abilities(); ab.instabuild = creative;
         set(Player.class, p, "abilities", ab);
         p.connection = (RecListener) U.allocateInstance(RecListener.class);
         final List<String> sync = new ArrayList<>();
         syncLines = sync;
         InventoryMenu menu = new InventoryMenu(inv, true, p);
         menu.setSynchronizer(new ContainerSynchronizer() {
            public void sendInitialData(AbstractContainerMenu c, List<ItemStack> items, ItemStack carried, int[] data) { c.incrementStateId(); }
            public void sendSlotChange(AbstractContainerMenu c, int i, ItemStack s) { sync.add("SYNC slot " + c.incrementStateId() + " " + i + " " + st(s)); }
            public void sendCarriedChange(AbstractContainerMenu c, ItemStack s) { }
            public void sendDataChange(AbstractContainerMenu c, int id, int v) { }
            public RemoteSlot createSlot() { return new RemoteSlot.Synchronized(comp -> 0); }
         });
         sync.clear();
         set(Player.class, p, "inventoryMenu", menu);
         set(Player.class, p, "containerMenu", menu);
         op("scenario " + scenario + " creative=" + creative);
         for (int step = 0; step < 90; step++) {
            int kind = r.nextInt(20);
            if (kind < 7) {
               int slot = r.nextInt(36);
               String name = PALETTE[r.nextInt(PALETTE.length - 3)];
               int count = 1 + r.nextInt(Math.max(1, new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(name))).getMaxStackSize()));
               op("inv " + slot + " " + name + " " + count);
               inv.setItem(slot, new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(name)), count));
            } else if (kind < 8) {
               int slot = 1 + r.nextInt(4);
               String name = PALETTE[r.nextInt(PALETTE.length)];
               int count = 1 + r.nextInt(8);
               op("grid " + slot + " " + name + " " + count);
               menu.getSlot(slot).set(new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(name)), count));
            } else if (kind < 9) {
               op("clearinv");
               for (int i = 0; i < 36; i++) inv.setItem(i, ItemStack.EMPTY);
            } else {
               RecipeHolder<?> h = candidates.get(r.nextInt(candidates.size()));
               boolean useMax = r.nextBoolean();
               op("place " + h.id().identifier() + " " + useMax);
               var action = menu.handlePlacement(useMax, p.isCreative(), h, lvl, inv);
               dump(menu, p, action.name());
               continue;
            }
            dump(menu, p, "-");
         }
      }
   }
}
