// Golden generator for crates/mc-game/tests/golden/recipe_golden.rs: the REAL vanilla RecipeManager over the vanilla data pack.
//   scripts/golden-recipes.sh   (runs this against the vanilla classpath and writes recipe_golden.txt)
// Output (tab separated):
//   RECIPE id type special group category notification placement          every recipe in registry order
//   DISPLAY index recipeId hex                                            RecipeDisplayEntry.STREAM_CODEC bytes
//   PROPSET key items...                                                  sorted item names of a synchronized property set
//   STONECUT inputHex displayHex                                          stonecutter entries in order
//   MATCH type shape stacks matchedIds firstResult remaining       scripted inputs (stacks joined by @@; remaining only for crafting)
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
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
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
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
      while (buf.isReadable()) b.append(String.format("%02x", buf.readByte() & 0xff));
      return b.toString();
   }

   static void line(String... f) { OUT.append(String.join("\t", f)).append('\n'); }

   static String ingredientItems(Ingredient i) {
      StringBuilder b = new StringBuilder();
      i.items().forEach(h -> b.append(b.length() > 0 ? "," : "").append(name(h.value())));
      return b.toString();
   }

   static ItemStack first(Ingredient i) { return new ItemStack(i.items().findFirst().get().value()); }
   static ItemStack nth(Ingredient i, int n) { return new ItemStack(i.items().skip(n).findFirst().orElse(i.items().findFirst().get()).value()); }

   static RecipeManager rm;
   static List<RecipeHolder<?>> all;

   static String joinStacks(List<ItemStack> l) {
      StringBuilder b = new StringBuilder();
      for (ItemStack s : l) b.append(b.length() > 0 ? "@@" : "").append(stack(s));
      return b.toString();
   }

   static <I extends RecipeInput> void match(RecipeType<?> type, String shape, List<ItemStack> stacks, I input) {
      List<String> ids = new ArrayList<>();
      String result = "{}";
      String remaining = "";
      for (RecipeHolder<?> h : all) {
         if (h.value().getType() != type) continue;
         boolean m;
         try { m = ((Recipe<I>) h.value()).matches(input, null); } catch (NullPointerException e) { continue; }
         if (m) {
            if (ids.isEmpty()) {
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

   public static void main(String[] args) throws Exception {
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
      access = loaded.layers().compositeAccess();
      var comps = BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(loaded.lookupWithUpdatedTags());
      comps.forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
      nbtOps = access.createSerializationContext(NbtOps.INSTANCE);

      rm = new RecipeManager(loaded.lookupWithUpdatedTags());
      rm.finalizeRecipeLoading(FeatureFlags.VANILLA_SET);
      all = new ArrayList<>(rm.getRecipes());

      for (RecipeHolder<?> h : all) {
         Recipe<?> r = h.value();
         PlacementInfo p = r.placementInfo();
         StringBuilder pl = new StringBuilder();
         pl.append(p.slotsToIngredientIndex().toString()).append(';');
         for (Ingredient i : p.ingredients()) pl.append(ingredientItems(i)).append('|');
         line("RECIPE", h.id().identifier().toString(), BuiltInRegistries.RECIPE_TYPE.getKey(r.getType()).toString(), String.valueOf(r.isSpecial()), r.group(),
            BuiltInRegistries.RECIPE_BOOK_CATEGORY.getKey((RecipeBookCategory) r.recipeBookCategory()).toString(), String.valueOf(r.showNotification()), pl.toString());
      }
      int[] index = {0};
      for (RecipeHolder<?> h : all) {
         rm.listDisplaysForRecipe(h.id(), entry -> line("DISPLAY", String.valueOf(index[0]++), h.id().identifier().toString(),
            hex(b -> RecipeDisplayEntry.STREAM_CODEC.encode(b, entry))));
      }
      for (var e : new TreeMap<>(rm.getSynchronizedItemProperties().entrySet().stream().collect(java.util.stream.Collectors.toMap(x -> x.getKey().identifier().toString(), x -> x.getValue()))).entrySet()) {
         List<String> names = new ArrayList<>();
         for (Item i : BuiltInRegistries.ITEM) if (e.getValue().test(new ItemStack(i))) names.add(name(i));
         Collections.sort(names);
         line("PROPSET", e.getKey(), String.join(" ", names));
      }
      for (var entry : rm.getSynchronizedStonecutterRecipes().entries()) {
         line("STONECUT", hex(b -> Ingredient.CONTENTS_STREAM_CODEC.encode(b, entry.input())), hex(b -> SlotDisplay.STREAM_CODEC.encode(b, entry.recipe().optionDisplay())));
      }
      try { Cases.run(); } catch (Throwable t) { System.err.println("FAIL " + t.getClass().getName()); for (StackTraceElement e : t.getStackTrace()) System.err.println("  at " + e); Throwable c = t.getCause(); while (c != null) { System.err.println("cause " + c.getClass().getName() + " " + c.getMessage()); c = c.getCause(); } System.exit(1); }
      System.out.print(OUT);
   }

   /** Scripted match inputs derived from the recipes themselves. */
   static class Cases {
      static void run() {
         ItemStack dirt = new ItemStack(Items.DIRT);
         ItemStack stick = new ItemStack(Items.STICK);
         for (RecipeHolder<?> h : all) {
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
         // shield decoration
         grid(S(banner("green", p1 + "," + p2)), S("{id:\"minecraft:shield\",count:1}"));
         grid(S("{id:\"minecraft:shield\",count:1}"), S(banner("green", null)));
         grid(S(banner("green", p1)), S("{id:\"minecraft:shield\",count:1,components:{banner_patterns:[" + p2 + "]}}"));
         // decorated pot
         String brick = "{id:\"minecraft:brick\",count:1}", sherd = "{id:\"minecraft:arms_up_pottery_sherd\",count:1}";
         grid(ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(sherd), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(sherd));
         grid(ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick));
         grid(ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(brick), ItemStack.EMPTY, S(sherd), ItemStack.EMPTY, S(brick), S(brick));
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
         grid(S(String.format(pick, 100, "")), S(String.format(pick, 50, "")));
         grid(S(String.format(pick, 100, ",enchantments:{\"minecraft:vanishing_curse\":1,\"minecraft:efficiency\":3}")), S(String.format(pick, 200, ",enchantments:{\"minecraft:binding_curse\":1,\"minecraft:vanishing_curse\":1}")));
         grid(S(String.format(pick, 100, "")), S("{id:\"minecraft:iron_axe\",count:1,components:{damage:5}}"));
         grid(S(String.format(pick, 100, "")), S(String.format(pick, 50, "")), S(String.format(pick, 50, "")));
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
         // map extending: the shape matches but the map data needs a level (not available in this oracle): only the shape
         String map = "{id:\"minecraft:filled_map\",count:1,components:{map_id:5}}";
         grid(S(paper), S(paper), S(paper), S(paper), S(map), S(paper), S(paper), S(paper), S(paper));
         // transmute with material counts (tipped arrows share the imbue path; copper tools etc. via placement above)
         grid(S("{id:\"minecraft:white_shulker_box\",count:1}"), S("{id:\"minecraft:white_shulker_box\",count:1}"));
      }
   }
}
