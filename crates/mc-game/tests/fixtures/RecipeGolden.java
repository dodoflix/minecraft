// Golden generator for crates/mc-game/tests/golden/recipe_golden.rs: the REAL vanilla RecipeManager over the vanilla data pack.
// Usage: scripts/golden-recipes.sh (the class takes FIXTURES OUT: FIXTURES holds the recipe packs `recipes`, `recipes_invalid` and `recipes_duplicate`, OUT is the file written because the server logs share stdout)
// Output: crates/mc-game/tests/fixtures/recipe_golden.txt.gz
// Format (tab separated):
//   RECIPE id type special group category notification placement          every recipe in registry order
//   DISPLAY index recipeId hex                                            RecipeDisplayEntry.STREAM_CODEC bytes
//   PROPSET key items...                                                  sorted item names of a synchronized property set
//   STONECUT inputHex displayHex                                          stonecutter entries in order
//   MATCH type shape stacks matchedIds firstResult remaining       scripted inputs (stacks joined by @@; remaining only for crafting)
//   MAPDATA id scale                                              the saved data of the level the MATCH rows ran against (CACHE rows change it with their own `scale` ops)
//   SKIP id / COUNT recipes displays stonecutter                  recipes that cannot be placed (logged, left out of the sets) / sizes
//   UPDATE hex                                                    ClientboundUpdateRecipesPacket (item set order differs per JVM run: compare decoded)
//   FLAGS set displays stonecutter / FPROPSET set key count       finalizeRecipeLoading with other feature sets
//   CACHE capacity op a b result                                  RecipeCache script (get: shape stacks id|-; scale: map id scale; swap/base: recipe manager)
//   P-prefixed copies of all of the above (only recipes of the pack) for a world with the pack `recipes`, ERR pack registry/id: message of the report / FAILURE pack class message of the bare exception, for `recipes_invalid` / `recipes_duplicate`
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.lang.reflect.Field;
import java.util.*;
import java.util.concurrent.Executor;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.ReportedException;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackResources;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.PathPackResources;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.item.crafting.display.*;

public class RecipeGolden {
   static final StringBuilder OUT = new StringBuilder();
   static RegistryAccess.Frozen access;
   static RegistryOps<Tag> nbtOps;

   static String name(Item i) { return BuiltInRegistries.ITEM.getKey(i).toString(); }

   static String stack(ItemStack s) {
      if (s.isEmpty()) return "{}";
      return ItemStack.OPTIONAL_CODEC.encodeStart(nbtOps, s).getOrThrow().toString();
   }

   static String hex(java.util.function.Consumer<RegistryFriendlyByteBuf> w) {
      RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
      w.accept(buf);
      StringBuilder b = new StringBuilder();
      while (buf.isReadable()) b.append(String.format(java.util.Locale.ROOT, "%02x", buf.readByte() & 0xff));
      return b.toString();
   }

   static void line(String... f) { f[0] = P + f[0]; OUT.append(String.join("\t", f)).append('\n'); }

   static String ingredientItems(Ingredient i) {
      StringBuilder b = new StringBuilder();
      i.items().forEach(h -> b.append(b.length() > 0 ? "," : "").append(name(h.value())));
      return b.toString();
   }

   static ItemStack first(Ingredient i) { return new ItemStack(i.items().findFirst().get().value()); }
   static ItemStack nth(Ingredient i, int n) { return new ItemStack(i.items().skip(n).findFirst().orElse(i.items().findFirst().get()).value()); }

   static RecipeManager rm;
   static List<RecipeHolder<?>> all;
   static HolderLookup.Provider lookup;
   static boolean componentsApplied;
   /** Tag prefix of the lines of the data pack world ("P") or empty for the vanilla data. */
   static String P = "";
   /** What `FakeLevel.recipeAccess()` returns. */
   static RecipeManager levelRecipes;
   /** Map id -> `MapItemSavedData.scale` of the fake level (absent = no saved data). */
   static final Map<Integer, Integer> SCALES = new TreeMap<>();
   static ServerLevel LEVEL;
   /** Which matching recipes may supply the assembled result (the first one passing); the ids always list every match. */
   static java.util.function.Predicate<String> resultFrom = id -> true;

   /** A `ServerLevel` that only knows its recipe manager and map data, like the level the recipe code needs. */
   public static class FakeLevel extends ServerLevel {
      public FakeLevel() { super(null, null, null, null, null, null, false, 0L, null, false); }
      @Override public RecipeManager recipeAccess() { return levelRecipes; }
      @Override public net.minecraft.world.level.saveddata.maps.MapItemSavedData getMapData(net.minecraft.world.level.saveddata.maps.MapId id) {
         Integer scale = SCALES.get(id.id());
         return scale == null ? null : net.minecraft.world.level.saveddata.maps.MapItemSavedData.createFresh(0, 0, (byte) (int) scale, false, false, net.minecraft.world.level.Level.OVERWORLD);
      }
   }

   static String joinStacks(List<ItemStack> l) {
      StringBuilder b = new StringBuilder();
      for (ItemStack s : l) b.append(b.length() > 0 ? "@@" : "").append(stack(s));
      return b.toString();
   }

   static <I extends RecipeInput> void match(RecipeType<?> type, String shape, List<ItemStack> stacks, I input) {
      List<String> ids = new ArrayList<>();
      String result = "{}";
      String remaining = "";
      boolean resultTaken = false;
      for (RecipeHolder<?> h : all) {
         if (h.value().getType() != type) continue;
         if (((Recipe<I>) h.value()).matches(input, LEVEL)) {
            if (!resultTaken && resultFrom.test(h.id().identifier().toString())) {
               resultTaken = true;
               result = stack(((Recipe<I>) h.value()).assemble(input));
               if (input instanceof CraftingInput ci && h.value() instanceof CraftingRecipe cr) remaining = joinStacks(new ArrayList<>(cr.getRemainingItems(ci)));
            }
            ids.add(h.id().identifier().toString());
         }
      }
      line("MATCH", BuiltInRegistries.RECIPE_TYPE.getKey(type).toString(), shape, joinStacks(stacks), String.join(",", ids), result, remaining);
   }

   static void crafting(int w, int h, List<ItemStack> grid) {
      CraftingInput in = CraftingInput.of(w, h, grid);
      match(RecipeType.CRAFTING, w + "x" + h, grid, in);
   }

   static final Executor DIRECT = Runnable::run;

   static LayeredRegistryAccess<RegistryLayer> worldLayers;

   /** The reloadable registries (recipes included) of the vanilla data plus `extra` packs, like startup the first time and like `/reload` (world registries kept) afterwards; throws when a recipe fails to load. */
   static ReloadableServerRegistries.LoadResult load(List<PackResources> extra) {
      List<PackResources> packs = new ArrayList<>();
      packs.add(ServerPacksSource.createVanillaPackSource().fullResources());
      packs.addAll(extra);
      MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, packs);
      LayeredRegistryAccess<RegistryLayer> layers = worldLayers != null ? worldLayers : RegistryLayer.createRegistryAccess();
      List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
      staticTags.forEach(Registry.PendingTags::apply);
      if (worldLayers == null) {
         var worldContext = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
         var world = RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, DIRECT).join();
         List<HolderLookup.RegistryLookup<?>> dimContext = new ArrayList<>(worldContext);
         world.listRegistries().forEach(dimContext::add);
         var dims = RegistryDataLoader.load(resources, dimContext, RegistryDataLoader.DIMENSION_REGISTRIES, DIRECT).join();
         layers = layers.replaceFrom(RegistryLayer.WORLD, world, dims);
         worldLayers = layers;
      }
      return ReloadableServerRegistries.reload(layers, staticTags, resources, DIRECT).join();
   }

   static void install(ReloadableServerRegistries.LoadResult loaded) {
      access = loaded.layers().compositeAccess();
      lookup = loaded.lookupWithUpdatedTags();
      if (!componentsApplied) {
         BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
         componentsApplied = true;
      }
      nbtOps = access.createSerializationContext(NbtOps.INSTANCE);
      rm = manager(FeatureFlags.VANILLA_SET);
      levelRecipes = rm;
      all = new ArrayList<>(rm.getRecipes());
   }

   static RecipeManager manager(FeatureFlagSet enabled) {
      RecipeManager m = new RecipeManager(lookup);
      m.finalizeRecipeLoading(enabled);
      return m;
   }

   static int displayCount(RecipeManager m) {
      int[] n = {0};
      for (RecipeHolder<?> h : m.getRecipes()) m.listDisplaysForRecipe(h.id(), e -> n[0]++);
      return n[0];
   }

   /** RECIPE / DISPLAY / PROPSET / STONECUT / SKIP / COUNT / UPDATE lines of the current world, the recipe lines only for ids passing `ids`. */
   static void dump(java.util.function.Predicate<String> ids) {
      for (RecipeHolder<?> h : all) {
         Recipe<?> r = h.value();
         PlacementInfo p = r.placementInfo();
         if (!r.isSpecial() && p.isImpossibleToPlace()) line("SKIP", h.id().identifier().toString());
         if (!ids.test(h.id().identifier().toString())) continue;
         StringBuilder pl = new StringBuilder();
         pl.append(p.slotsToIngredientIndex().toString()).append(';');
         for (Ingredient i : p.ingredients()) pl.append(ingredientItems(i)).append('|');
         line("RECIPE", h.id().identifier().toString(), BuiltInRegistries.RECIPE_TYPE.getKey(r.getType()).toString(), String.valueOf(r.isSpecial()), r.group(),
            BuiltInRegistries.RECIPE_BOOK_CATEGORY.getKey((RecipeBookCategory) r.recipeBookCategory()).toString(), String.valueOf(r.showNotification()), pl.toString());
      }
      int[] index = {0};
      for (RecipeHolder<?> h : all) {
         rm.listDisplaysForRecipe(h.id(), entry -> {
            int i = index[0]++;
            if (ids.test(h.id().identifier().toString())) line("DISPLAY", String.valueOf(i), h.id().identifier().toString(), hex(b -> RecipeDisplayEntry.STREAM_CODEC.encode(b, entry)));
         });
      }
      for (var e : new TreeMap<>(rm.getSynchronizedItemProperties().entrySet().stream().collect(java.util.stream.Collectors.toMap(x -> x.getKey().identifier().toString(), x -> x.getValue()))).entrySet()) {
         List<String> names = new ArrayList<>();
         for (Item i : BuiltInRegistries.ITEM) if (e.getValue().test(new ItemStack(i))) names.add(name(i));
         Collections.sort(names);
         line("PROPSET", e.getKey(), String.join(" ", names));
      }
      int stonecut = 0;
      for (var entry : rm.getSynchronizedStonecutterRecipes().entries()) {
         stonecut++;
         if (ids.test(entry.recipe().recipe().get().id().identifier().toString()))
            line("STONECUT", hex(b -> Ingredient.CONTENTS_STREAM_CODEC.encode(b, entry.input())), hex(b -> SlotDisplay.STREAM_CODEC.encode(b, entry.recipe().optionDisplay())));
      }
      line("COUNT", String.valueOf(all.size()), String.valueOf(displayCount(rm)), String.valueOf(stonecut));
      // the item sets (unmodifiable maps and sets: wire order differs per JVM run) and the stonecutter list as one packet
      var packet = new net.minecraft.network.protocol.game.ClientboundUpdateRecipesPacket(rm.getSynchronizedItemProperties(), rm.getSynchronizedStonecutterRecipes());
      line("UPDATE", hex(b -> net.minecraft.network.protocol.game.ClientboundUpdateRecipesPacket.STREAM_CODEC.encode(b, packet)));
   }

   /** `finalizeRecipeLoading` with feature sets that are not vanilla's: every item requires the `vanilla` flag. */
   static void flags() {
      Object[][] sets = {
         {"empty", FeatureFlagSet.of()},
         {"trade_rebalance", FeatureFlags.REGISTRY.subset(FeatureFlags.TRADE_REBALANCE)},
         {"vanilla", FeatureFlags.VANILLA_SET},
         {"all", FeatureFlags.REGISTRY.allFlags()},
      };
      for (Object[] s : sets) {
         RecipeManager m = manager((FeatureFlagSet) s[1]);
         line("FLAGS", (String) s[0], String.valueOf(displayCount(m)), String.valueOf(m.getSynchronizedStonecutterRecipes().entries().size()));
         for (var e : new TreeMap<>(m.getSynchronizedItemProperties().entrySet().stream().collect(java.util.stream.Collectors.toMap(x -> x.getKey().identifier().toString(), x -> x.getValue()))).entrySet()) {
            int n = 0;
            for (Item i : BuiltInRegistries.ITEM) if (e.getValue().test(new ItemStack(i))) n++;
            line("FPROPSET", (String) s[0], e.getKey(), String.valueOf(n));
         }
      }
   }

   static void cacheOp(int cap, String op, String a, String b, String result) { line("CACHE", String.valueOf(cap), op, a, b, result); }

   /** `RecipeCache` hits and misses, seen through map data the cached result depends on (a hit keeps the stale answer). */
   static void cache() {
      RecipeManager other = manager(FeatureFlags.VANILLA_SET);
      String map = "{id:\"minecraft:filled_map\",count:1,components:{map_id:1}}";
      Map<String, List<ItemStack>> grids = new LinkedHashMap<>();
      List<ItemStack> a = new ArrayList<>();
      for (int i = 0; i < 9; i++) a.add(i == 4 ? Cases.S(map) : new ItemStack(Items.PAPER));
      grids.put("A", a);
      grids.put("B", List.of(new ItemStack(Items.OAK_PLANKS)));
      grids.put("C", Collections.nCopies(4, new ItemStack(Items.OAK_PLANKS)));
      grids.put("E", Collections.nCopies(9, ItemStack.EMPTY));
      Map<String, String> shapes = Map.of("A", "3x3", "B", "1x1", "C", "2x2", "E", "3x3");
      String[][] scripts = {
         {"1", "scale 1 0", "get A", "scale 1 4", "get A", "get B", "get A", "scale 1 0", "get A", "get E", "get A", "swap", "get A", "scale 1 4", "base", "get A"},
         {"2", "scale 1 0", "get A", "get B", "get A", "get C", "scale 1 4", "get A", "get B", "get A", "get C", "get A", "get B"},
         {"3", "scale 1 0", "get C", "get B", "get A", "scale 1 4", "get C", "get A", "get B", "get C", "get A", "scale 1 0", "get B", "get A"},
      };
      for (String[] script : scripts) {
         int cap = Integer.parseInt(script[0]);
         RecipeCache cache = new RecipeCache(cap);
         levelRecipes = rm;
         for (int i = 1; i < script.length; i++) {
            String[] w = script[i].split(" ");
            switch (w[0]) {
               case "scale" -> { SCALES.put(Integer.parseInt(w[1]), Integer.parseInt(w[2])); cacheOp(cap, "scale", w[1], w[2], "-"); }
               case "swap" -> { levelRecipes = other; cacheOp(cap, "swap", "-", "-", "-"); }
               case "base" -> { levelRecipes = rm; cacheOp(cap, "base", "-", "-", "-"); }
               default -> {
                  List<ItemStack> g = grids.get(w[1]);
                  String[] sh = shapes.get(w[1]).split("x");
                  CraftingInput in = CraftingInput.of(Integer.parseInt(sh[0]), Integer.parseInt(sh[1]), g);
                  cacheOp(cap, "get", shapes.get(w[1]), joinStacks(g), cache.get(LEVEL, in).map(r -> r.id().identifier().toString()).orElse("-"));
               }
            }
         }
      }
      levelRecipes = rm;
   }

   static PackResources packOf(java.nio.file.Path dir, String id) {
      return new PathPackResources(new PackLocationInfo(id, net.minecraft.network.chat.Component.literal(id), PackSource.WORLD, Optional.empty()), dir);
   }

   /** A world with the pack `recipes` (one custom recipe per serializer plus recipes that cannot be placed) and the failures of `recipes_invalid` and `recipes_duplicate`. */
   static void pack(java.nio.file.Path fixtures) {
      P = "P";
      install(load(List.of(packOf(fixtures.resolve("recipes"), "file/recipes"))));
      java.util.function.Predicate<String> mine = id -> id.startsWith("test:") || id.equals("minecraft:stick");
      resultFrom = mine;
      dump(mine);
      for (var e : SCALES.entrySet()) line("MAPDATA", String.valueOf(e.getKey()), String.valueOf(e.getValue()));
      Cases.run(h -> mine.test(h.id().identifier().toString()));
      resultFrom = id -> true;
      for (String bad : new String[] {"recipes_invalid", "recipes_duplicate"}) {
         try {
            load(List.of(packOf(fixtures.resolve(bad), "file/" + bad)));
            throw new IllegalStateException("the pack " + bad + " loaded");
         } catch (java.util.concurrent.CompletionException e) {
            if (e.getCause() instanceof ReportedException re) {
               for (String l : re.getReport().getDetails().split("\n")) if (l.startsWith("\t\t")) line("ERR", bad, l.substring(2));
            } else {
               // a registry that refuses a value it already holds (the unit serializers of the special recipes): no report, the exception itself
               line("FAILURE", bad, e.getCause().getClass().getName(), e.getCause().getMessage().replaceAll("@[0-9a-f]+", "@"));
            }
         }
      }
      P = "";
   }

   public static void main(String[] args) throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      uf.setAccessible(true);
      LEVEL = (ServerLevel) ((sun.misc.Unsafe) uf.get(null)).allocateInstance(FakeLevel.class);
      install(load(List.of()));
      SCALES.put(1, 0);
      SCALES.put(2, 3);
      SCALES.put(3, 4);
      dump(id -> true);
      for (var e : SCALES.entrySet()) line("MAPDATA", String.valueOf(e.getKey()), String.valueOf(e.getValue()));
      try {
         Cases.run(h -> true);
         flags();
         cache();
         pack(java.nio.file.Path.of(args[0]));
      } catch (Throwable t) { System.err.println("FAIL " + t.getClass().getName() + " " + t.getMessage()); for (StackTraceElement e : t.getStackTrace()) System.err.println("  at " + e); Throwable c = t.getCause(); while (c != null) { System.err.println("cause " + c.getClass().getName() + " " + c.getMessage()); c = c.getCause(); } System.exit(1); }
      java.nio.file.Files.writeString(java.nio.file.Path.of(args[1]), OUT.toString());
   }

   /** Scripted match inputs derived from the recipes themselves. */
   static class Cases {
      static void run(java.util.function.Predicate<RecipeHolder<?>> derive) {
         ItemStack dirt = new ItemStack(Items.DIRT);
         ItemStack stick = new ItemStack(Items.STICK);
         for (RecipeHolder<?> h : all) {
            if (!derive.test(h)) continue;
            Recipe<?> r = h.value();
            if (r instanceof ShapedRecipe sr) {
               List<Optional<Ingredient>> ing = sr.getIngredients();
               int w = sr.getWidth(), hh = sr.getHeight();
               for (int variant = 0; variant < 4; variant++) {
                  List<ItemStack> grid = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
                  int offX = variant == 1 ? 3 - w : 0, offY = variant == 1 ? 3 - hh : 0;
                  boolean flip = variant == 2;
                  for (int y = 0; y < hh; y++) for (int x = 0; x < w; x++) {
                     Optional<Ingredient> o = ing.get((flip ? w - 1 - x : x) + y * w);
                     if (o.isPresent()) grid.set(x + offX + (y + offY) * 3, nth(o.get(), variant == 3 ? 1 : 0));
                  }
                  crafting(3, 3, grid);
               }
               List<ItemStack> bad = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
               boolean replaced = false;
               for (int y = 0; y < hh; y++) for (int x = 0; x < w; x++) {
                  Optional<Ingredient> o = ing.get(x + y * w);
                  if (o.isPresent()) { bad.set(x + y * 3, !replaced ? dirt : first(o.get())); replaced = true; }
               }
               crafting(3, 3, bad);
            } else if (r instanceof ShapelessRecipe) {
               List<Ingredient> ings = r.placementInfo().ingredients();
               if (ings.isEmpty()) continue;
               for (int variant = 0; variant < 3; variant++) {
                  List<ItemStack> grid = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
                  for (int i = 0; i < ings.size(); i++) {
                     int slot = variant == 1 ? 8 - i : i;
                     grid.set(slot, nth(ings.get(i), variant == 2 ? 1 : 0));
                  }
                  crafting(3, 3, grid);
               }
               List<ItemStack> extra = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
               for (int i = 0; i < ings.size(); i++) extra.set(i, first(ings.get(i)));
               if (ings.size() < 9) extra.set(8, stick);
               crafting(3, 3, extra);
               List<ItemStack> fewer = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
               for (int i = 0; i < ings.size() - 1; i++) fewer.set(i, first(ings.get(i)));
               crafting(3, 3, fewer);
            } else if (r instanceof DyeRecipe || r instanceof TransmuteRecipe) {
               List<Ingredient> ings = r.placementInfo().ingredients();
               if (ings.size() < 2) continue;
               for (int k = 1; k <= Math.min(ings.size() - 1, 4); k++) {
                  for (int variant = 0; variant < 2; variant++) {
                     List<ItemStack> grid = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
                     grid.set(variant == 0 ? 0 : 8, nth(ings.get(0), variant));
                     for (int j = 0; j < k; j++) grid.set(1 + j, nth(ings.get(1), j + variant));
                     crafting(3, 3, grid);
                  }
               }
            } else if (r instanceof ImbueRecipe) {
               List<Ingredient> ings = r.placementInfo().ingredients();
               List<ItemStack> grid = new ArrayList<>();
               for (int i = 0; i < ings.size(); i++) {
                  ItemStack s = first(ings.get(i));
                  if (i == 4) s.set(DataComponents.POTION_CONTENTS, new net.minecraft.world.item.alchemy.PotionContents(net.minecraft.world.item.alchemy.Potions.POISON));
                  grid.add(s);
               }
               crafting(3, 3, grid);
            } else if (r instanceof AbstractCookingRecipe cr) {
               var type = cr.getType();
               for (int n = 0; n < Math.min(3, (int) cr.input().items().count()); n++) {
                  ItemStack s = nth(cr.input(), n);
                  match(type, "1", List.of(s), new SingleRecipeInput(s));
               }
               match(type, "1", List.of(dirt), new SingleRecipeInput(dirt));
            } else if (r instanceof StonecutterRecipe sc) {
               ItemStack s = first(sc.input());
               match(RecipeType.STONECUTTING, "1", List.of(s), new SingleRecipeInput(s));
            } else if (r instanceof SmithingRecipe sm) {
               ItemStack t = sm.templateIngredient().map(Cases::firstOf).orElse(ItemStack.EMPTY);
               ItemStack b = firstOf(sm.baseIngredient());
               ItemStack a = sm.additionIngredient().map(Cases::firstOf).orElse(ItemStack.EMPTY);
               match(RecipeType.SMITHING, "3", List.of(t, b, a), new SmithingRecipeInput(t, b, a));
               match(RecipeType.SMITHING, "3", List.of(ItemStack.EMPTY, b, a), new SmithingRecipeInput(ItemStack.EMPTY, b, a));
            } else if (r instanceof BrewingRecipe br) {
               ItemStack in = potionStack(br.getInput());
               ItemStack re = potionStack(br.getReagent());
               match(RecipeType.BREWING, "2", List.of(in, re), new BrewingInput(in, re));
            }
         }
         extra();
      }

      static ItemStack firstOf(Ingredient i) { return first(i); }

      static ItemStack potionStack(PotionIngredient pi) {
         ItemStack s = first(pi.ingredient());
         pi.potions().ifPresent(p -> p.potions().ifPresent(set -> {
            if (set.size() > 0) s.set(DataComponents.POTION_CONTENTS, new net.minecraft.world.item.alchemy.PotionContents(set.get(0)));
         }));
         return s;
      }

      static ItemStack S(String snbt) {
         try {
            return ItemStack.OPTIONAL_CODEC.parse(nbtOps, TagParser.parseCompoundFully(snbt)).getOrThrow();
         } catch (Exception e) { throw new RuntimeException(snbt, e); }
      }

      static void grid(ItemStack... stacks) {
         List<ItemStack> g = new ArrayList<>(Collections.nCopies(9, ItemStack.EMPTY));
         for (int i = 0; i < stacks.length; i++) g.set(i, stacks[i]);
         crafting(3, 3, g);
      }

      static String book(int gen) { return "{id:\"minecraft:written_book\",count:1,components:{written_book_content:{title:\"T\",author:\"A\",generation:" + gen + ",pages:[\"p1\"]}}}"; }
      static String banner(String color, String patterns) { return "{id:\"minecraft:" + color + "_banner\",count:1" + (patterns == null ? "" : ",components:{banner_patterns:[" + patterns + "]}") + "}"; }
      static String star(String shape, String colors, String fade, boolean trail, boolean twinkle) {
         return "{id:\"minecraft:firework_star\",count:1,components:{firework_explosion:{shape:\"" + shape + "\",colors:[I;" + colors + "],fade_colors:[I;" + fade + "],has_trail:" + (trail ? "1b" : "0b") + ",has_twinkle:" + (twinkle ? "1b" : "0b") + "}}}";
      }

      static void extra() {
         // book cloning (allowed generations 0..1), remaining items keep the source book
         grid(S(book(0)), S("{id:\"minecraft:writable_book\",count:1}"));
         grid(S(book(1)), S("{id:\"minecraft:writable_book\",count:1}"), S("{id:\"minecraft:writable_book\",count:1}"), S("{id:\"minecraft:writable_book\",count:1}"));
         grid(S(book(2)), S("{id:\"minecraft:writable_book\",count:1}"));
         grid(S(book(0)), S(book(0)), S("{id:\"minecraft:writable_book\",count:1}"));
         grid(S("{id:\"minecraft:writable_book\",count:1}"), S("{id:\"minecraft:writable_book\",count:1}"));
         // banner duplicate: remaining keeps the patterned banner
         String p1 = "{pattern:\"minecraft:stripe_top\",color:\"red\"}", p2 = "{pattern:\"minecraft:creeper\",color:\"blue\"}";
         grid(S(banner("white", p1)), S(banner("white", null)));
         grid(S(banner("white", p1 + "," + p2)), S(banner("white", null)));
         grid(S(banner("white", p1)), S(banner("black", null)));
         grid(S(banner("white", p1)), S(banner("white", p2)));
         grid(S(banner("white", p1 + "," + p1 + "," + p1 + "," + p1 + "," + p1 + "," + p1 + "," + p1)), S(banner("white", null)));
         grid(S(banner("red", null)), S(banner("red", null)));
         grid(S(banner("cyan", p1)), S(banner("cyan", null)));
         // shield decoration
         grid(S(banner("green", p1 + "," + p2)), S("{id:\"minecraft:shield\",count:1}"));
         grid(S("{id:\"minecraft:shield\",count:1}"), S(banner("green", null)));
         grid(S(banner("green", p1)), S("{id:\"minecraft:shield\",count:1,components:{banner_patterns:[" + p2 + "]}}"));
         // decorated pot
         String brick = "{id:\"minecraft:brick\",count:1}", sherd = "{id:\"minecraft:arms_up_pottery_sherd\",count:1}";
         grid(ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(sherd), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(sherd));
         grid(ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick));
         grid(ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(sherd), ItemStack.EMPTY, S(brick), S(brick));
         grid(ItemStack.EMPTY, S("{id:\"minecraft:iron_ingot\",count:1}"), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick));
         // firework rocket
         String paper = "{id:\"minecraft:paper\",count:1}", gp = "{id:\"minecraft:gunpowder\",count:1}";
         grid(S(paper), S(gp));
         grid(S(paper), S(gp), S(star("star", "255,65280", "", true, false)), S(star("creeper", "16711680", "255", false, true)));
         grid(S(paper), S(gp), S(gp), S(gp), S(gp));
         grid(S(paper), S(paper), S(gp));
         grid(S(paper), S(gp), S("{id:\"minecraft:firework_star\",count:1}"));
         grid(S(paper), S("{id:\"minecraft:stick\",count:1}"), S(gp));
         // firework star
         String dye = "{id:\"minecraft:red_dye\",count:1}", dye2 = "{id:\"minecraft:lime_dye\",count:1}";
         grid(S(gp), S(dye));
         grid(S(gp), S(dye), S(dye2), S("{id:\"minecraft:fire_charge\",count:1}"), S("{id:\"minecraft:glowstone_dust\",count:1}"), S("{id:\"minecraft:diamond\",count:1}"));
         grid(S(gp), S(dye), S("{id:\"minecraft:gold_nugget\",count:1}"));
         grid(S(gp), S(dye), S("{id:\"minecraft:feather\",count:1}"));
         grid(S(gp), S(dye), S("{id:\"minecraft:creeper_head\",count:1}"));
         grid(S(gp), S(dye), S("{id:\"minecraft:fire_charge\",count:1}"), S("{id:\"minecraft:gold_nugget\",count:1}"));
         grid(S(gp), S(gp), S(dye));
         grid(S(gp), S("{id:\"minecraft:blue_dye\",count:1}"), S("{id:\"minecraft:glowstone_dust\",count:1}"), S("{id:\"minecraft:glowstone_dust\",count:1}"));
         // firework star fade
         grid(S(star("star", "255", "", false, false)), S(dye));
         grid(S(star("star", "255", "5", true, true)), S(dye), S(dye2));
         grid(S(star("star", "255", "", false, false)), S(star("star", "255", "", false, false)), S(dye));
         grid(S("{id:\"minecraft:firework_star\",count:1}"), S(dye));
         // repair item
         String pick = "{id:\"minecraft:iron_pickaxe\",count:1,components:{damage:%d%s}}";
         grid(S(String.format(java.util.Locale.ROOT, pick, 100, "")), S(String.format(java.util.Locale.ROOT, pick, 50, "")));
         grid(S(String.format(java.util.Locale.ROOT, pick, 100, ",enchantments:{\"minecraft:vanishing_curse\":1,\"minecraft:efficiency\":3}")), S(String.format(java.util.Locale.ROOT, pick, 200, ",enchantments:{\"minecraft:binding_curse\":1,\"minecraft:vanishing_curse\":1}")));
         grid(S(String.format(java.util.Locale.ROOT, pick, 100, "")), S("{id:\"minecraft:iron_axe\",count:1,components:{damage:5}}"));
         grid(S(String.format(java.util.Locale.ROOT, pick, 100, "")), S(String.format(java.util.Locale.ROOT, pick, 50, "")), S(String.format(java.util.Locale.ROOT, pick, 50, "")));
         grid(S("{id:\"minecraft:diamond_chestplate\",count:1,components:{damage:0,enchantments:{\"minecraft:binding_curse\":1}}}"), S("{id:\"minecraft:diamond_chestplate\",count:1,components:{damage:1000}}"));
         grid(S("{id:\"minecraft:stick\",count:1}"), S("{id:\"minecraft:stick\",count:1}"));
         // armor dye / dyed colors
         for (String target : new String[]{"leather_helmet", "leather_chestplate", "leather_leggings", "leather_boots", "leather_horse_armor", "wolf_armor"}) {
            String t = "{id:\"minecraft:" + target + "\",count:1}";
            grid(S(t), S(dye));
            grid(S(t), S(dye), S(dye2), S("{id:\"minecraft:blue_dye\",count:1}"));
            grid(S("{id:\"minecraft:" + target + "\",count:1,components:{dyed_color:11546150}}"), S("{id:\"minecraft:yellow_dye\",count:1}"), S("{id:\"minecraft:white_dye\",count:1}"));
            grid(S("{id:\"minecraft:" + target + "\",count:1,components:{dyed_color:0}}"), S("{id:\"minecraft:black_dye\",count:1}"));
         }
         for (String color : new String[]{"white", "black", "light_blue", "brown", "gray"}) {
            grid(S("{id:\"minecraft:shulker_box\",count:1}"), S("{id:\"minecraft:" + color + "_dye\",count:1}"));
            grid(S("{id:\"minecraft:" + color + "_shulker_box\",count:1,components:{custom_name:\"x\"}}"), S("{id:\"minecraft:red_dye\",count:1}"));
            grid(S("{id:\"minecraft:bundle\",count:1}"), S("{id:\"minecraft:" + color + "_dye\",count:1}"));
            grid(S("{id:\"minecraft:" + color + "_bundle\",count:1}"), S("{id:\"minecraft:red_dye\",count:1}"));
         }
         // imbue (tipped arrows)
         String arrow = "{id:\"minecraft:arrow\",count:1}";
         String lp = "{id:\"minecraft:lingering_potion\",count:1,components:{potion_contents:{potion:\"minecraft:poison\"}}}";
         grid(S(arrow), S(arrow), S(arrow), S(arrow), S(lp), S(arrow), S(arrow), S(arrow), S(arrow));
         grid(S(arrow), S(arrow), S(arrow), S(arrow), S("{id:\"minecraft:lingering_potion\",count:1}"), S(arrow), S(arrow), S(arrow), S(arrow));
         grid(S(arrow), S(arrow), S(arrow), S(arrow), S(lp), S(arrow), S(arrow), S(arrow));
         // map extending: the map's saved data decides (scale < 4); the result keeps the map's components
         for (int id = 1; id <= 4; id++) {
            String m = "{id:\"minecraft:filled_map\",count:1,components:{map_id:" + id + "}}";
            grid(S(paper), S(paper), S(paper), S(paper), S(m), S(paper), S(paper), S(paper), S(paper));
         }
         String named = "{id:\"minecraft:filled_map\",count:1,components:{map_id:2,custom_name:\"M\",repair_cost:3}}";
         grid(S(paper), S(paper), S(paper), S(paper), S(named), S(paper), S(paper), S(paper), S(paper));
         grid(S(paper), S(paper), S(paper), S(paper), S("{id:\"minecraft:filled_map\",count:1}"), S(paper), S(paper), S(paper), S(paper));
         grid(S(paper), S(paper), S(paper), S(paper), S("{id:\"minecraft:map\",count:1}"), S(paper), S(paper), S(paper), S(paper));
         grid(S(paper), S(paper), S(paper), S(paper), S("{id:\"minecraft:filled_map\",count:1,components:{map_id:1}}"), S(paper), S(paper), S(paper), S("{id:\"minecraft:stick\",count:1}"));
         grid(S(paper), S(paper), S(paper), S(paper), S("{id:\"minecraft:filled_map\",count:1,components:{map_id:1}}"), S(paper), S(paper), S(paper));
         // transmute with material counts (tipped arrows share the imbue path; copper tools etc. via placement above)
         grid(S("{id:\"minecraft:white_shulker_box\",count:1}"), S("{id:\"minecraft:white_shulker_box\",count:1}"));
      }
   }
}
