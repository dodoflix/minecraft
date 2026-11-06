import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.FeatureSorter;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.feature.FillLayerFeature;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;
import net.minecraft.world.level.levelgen.structure.Structure;

/**
 * Golden for the decoded feature/structure registries (Rust: tests/golden/vanilla_registries.rs).
 * Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/decoration/registries.txt crates/mc-game/tests/oracle/RegistryDecodeOracle.java crates/mc-game/tests/fixtures/decoration/registries.txt
 * Output: crates/mc-game/tests/fixtures/decoration/registries.txt
 *
 *   COUNT <registry> <n>                       (entries of the loaded registry)
 *   STRUCTURES <step ordinal> <id,id,...>       (Registry<Structure> iteration order grouped by Structure.step(), as applyBiomeDecoration)
 *   BIOME <id> <step> <placed feature,...>      (BiomeGenerationSettings.features() of every biome, in order)
 *   SORTED <overworld|the_nether|the_end> <steps> <size of step 0> <size of step 1> ...   (FeatureSorter over the dimension's biome source)
 */
public class RegistryDecodeOracle {
   public static void main(String[] args) throws Exception {
      RegistryAccess.Frozen access = extractor.Setup.bootstrap();
      Registry<PlacedFeature> placed = access.lookupOrThrow(Registries.PLACED_FEATURE);
      try (PrintWriter out = new PrintWriter(args[0])) {
         for (var key : new net.minecraft.resources.ResourceKey[]{Registries.BIOME, Registries.FEATURE, Registries.PLACED_FEATURE, Registries.STRUCTURE, Registries.STRUCTURE_SET, Registries.PROCESSOR_LIST, Registries.TEMPLATE_POOL}) {
            out.println("COUNT " + key.identifier() + " " + access.lookupOrThrow(key).size());
         }
         Registry<Structure> structures = access.lookupOrThrow(Registries.STRUCTURE);
         for (int step = 0; step < net.minecraft.world.level.levelgen.GenerationStep.Decoration.values().length; step++) {
            final int s = step;
            List<String> ids = new ArrayList<>();
            structures.stream().filter(st -> st.step().ordinal() == s).forEach(st -> ids.add(structures.getKey(st).toString()));
            out.println("STRUCTURES " + step + " " + String.join(",", ids));
         }
         Registry<Biome> biomes = access.lookupOrThrow(Registries.BIOME);
         for (Holder.Reference<Biome> biome : biomes.listElements().toList()) {
            List<HolderSet<PlacedFeature>> features = biome.value().getGenerationSettings().features();
            for (int step = 0; step < features.size(); step++) {
               List<String> names = new ArrayList<>();
               for (Holder<PlacedFeature> h : features.get(step)) {
                  names.add(name(placed, h.value()));
               }
               out.println("BIOME " + biome.key().identifier() + " " + step + " " + String.join(",", names));
            }
         }
         var dims = access.lookupOrThrow(Registries.WORLD_PRESET).getValue(Identifier.withDefaultNamespace("normal")).createWorldDimensions().dimensions();
         for (String dim : new String[]{"overworld", "the_nether", "the_end"}) {
            LevelStem stem = dims.entrySet().stream().filter(e -> e.getKey().identifier().getPath().equals(dim)).findFirst().orElseThrow().getValue();
            var generator = stem.generator();
            List<Holder<Biome>> possible = List.copyOf(generator.getBiomeSource().possibleBiomes());
            List<FeatureSorter.StepFeatureData> steps = FeatureSorter.buildFeaturesPerStep(possible, b -> generator.getBiomeGenerationSettings(b).features(), true);
            StringBuilder sb = new StringBuilder("SORTED " + dim + " " + steps.size());
            for (FeatureSorter.StepFeatureData step : steps) {
               sb.append(' ').append(step.features().size());
            }
            out.println(sb);
         }
      }
   }

   static String name(Registry<PlacedFeature> placed, PlacedFeature feature) {
      if (feature.feature().value() instanceof FillLayerFeature fill) {
         return "FILL_LAYER:" + fill.height();
      }
      return placed.getResourceKey(feature).orElseThrow().identifier().toString();
   }
}
