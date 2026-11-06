import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.TreeSet;
import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.NoiseColumn;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.WorldGenLevel;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeGenerationSettings;
import net.minecraft.world.level.biome.BiomeSource;
import net.minecraft.world.level.biome.FeatureSorter;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.FlatLevelSource;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.feature.Feature;
import net.minecraft.world.level.levelgen.feature.FillLayerFeature;
import net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;

/**
 * Goldens for FeatureSorter and ChunkGenerator.applyBiomeDecoration (the Rust side: levelgen/decoration.rs, crates/mc-game-core/tests/golden/decoration_golden.rs).
 * Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/decoration/decoration.txt crates/mc-game/tests/oracle/DecorationOracle.java crates/mc-game/tests/fixtures/decoration/decoration.txt
 * Output: crates/mc-game/tests/fixtures/decoration/decoration.txt
 *
 * Per generator (vanilla overworld/nether/end level stems and flat settings):
 *   GEN <name> <flat settings json | ->
 *   FEATURES <name> <step> <feature,feature,...>       (FeatureSorter.buildFeaturesPerStep over the biome source, in order)
 * and decoration runs of the real applyBiomeDecoration (every placed feature replaced by a recording stand-in that is
 * placed without placement modifiers):
 *   CASE <name> <seed> <chunkX> <chunkZ> <biomes in the 3x3 chunk palettes>
 *   DECSEED <decoration seed>
 *   PLACE <feature> <first long of the feature's random>      (in placement order)
 */
public class DecorationOracle {
   static RegistryAccess.Frozen access;
   static RegistryOps<JsonElement> ops;
   static Registry<Biome> biomeRegistry;
   static Registry<PlacedFeature> placedRegistry;
   static List<String> placed = new ArrayList<>();
   static int minY, height;

   static String name(PlacedFeature feature) {
      if (feature.feature().value() instanceof FillLayerFeature fill) {
         return "FILL_LAYER:" + fill.height() + ":" + BuiltInRegistries.BLOCK.getKey(fill.state().getBlock());
      }
      return placedRegistry.getResourceKey(feature).orElseThrow().identifier().toString();
   }

   static final class Recording implements Feature {
      final String name;

      Recording(String name) {
         this.name = name;
      }

      @Override
      public MapCodec<? extends Feature> codec() {
         throw new UnsupportedOperationException();
      }

      @Override
      public boolean place(WorldGenLevel level, ChunkGenerator generator, RandomSource random, BlockPos origin) {
         placed.add(name + " " + random.nextLong() + " " + origin.getX() + "," + origin.getY() + "," + origin.getZ());
         return false;
      }
   }

   static final String[] FLATS = {
      "{\"layers\":[{\"block\":\"minecraft:bedrock\",\"height\":1},{\"block\":\"minecraft:dirt\",\"height\":2},{\"block\":\"minecraft:grass_block\",\"height\":1}],\"biome\":\"minecraft:plains\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:bedrock\",\"height\":1},{\"block\":\"minecraft:stone\",\"height\":10},{\"block\":\"minecraft:water\",\"height\":3},{\"block\":\"minecraft:glass\",\"height\":2},{\"block\":\"minecraft:snow\",\"height\":1},{\"block\":\"minecraft:short_grass\",\"height\":1}],\"biome\":\"minecraft:ocean\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1},{\"block\":\"minecraft:air\",\"height\":2},{\"block\":\"minecraft:lava\",\"height\":2},{\"block\":\"minecraft:stone\",\"height\":0},{\"block\":\"minecraft:oak_leaves\",\"height\":1},{\"block\":\"minecraft:oak_slab\",\"height\":1}],\"biome\":\"minecraft:desert\",\"features\":true}",
      "{\"layers\":[{\"block\":\"minecraft:air\",\"height\":3}],\"biome\":\"minecraft:the_void\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:bedrock\",\"height\":1},{\"block\":\"minecraft:netherrack\",\"height\":100},{\"block\":\"minecraft:lava\",\"height\":5}],\"biome\":\"minecraft:nether_wastes\",\"features\":true,\"lakes\":false}",
      "{\"layers\":[{\"block\":\"minecraft:stone\",\"height\":1}],\"biome\":\"minecraft:plains\"}",
   };

   static ChunkGenerator standIn(ChunkGenerator real) {
      Map<PlacedFeature, PlacedFeature> map = new IdentityHashMap<>();
      return new ChunkGenerator(real.getBiomeSource(), biome -> {
         BiomeGenerationSettings.PlainBuilder builder = new BiomeGenerationSettings.PlainBuilder();
         List<HolderSet<PlacedFeature>> features = real.getBiomeGenerationSettings(biome).features();
         for (int step = 0; step < features.size(); step++) {
            for (Holder<PlacedFeature> h : features.get(step)) {
               PlacedFeature pf = h.value();
               builder.addFeature(step, Holder.direct(map.computeIfAbsent(pf, k -> new PlacedFeature(Holder.direct(new Recording(name(k))), List.of()))));
            }
         }
         return builder.build();
      }) {
         @Override
         protected MapCodec<? extends ChunkGenerator> codec() {
            throw new UnsupportedOperationException();
         }

         @Override
         public void spawnOriginalMobs(net.minecraft.server.level.WorldGenRegion region) {
         }

         @Override
         public int getGenDepth() {
            return real.getGenDepth();
         }

         @Override
         public CompletableFuture<ChunkAccess> buildTerrain(ChunkAccess chunk, net.minecraft.world.level.levelgen.blending.Blender blender,
            net.minecraft.world.level.levelgen.RandomState randomState, StructureManager structureManager, net.minecraft.world.level.biome.BiomeManager biomeManager,
            net.minecraft.server.level.WorldGenRegion carverBiomeRegion, Set<Holder<Biome>> possibleBiomes) {
            throw new UnsupportedOperationException();
         }

         @Override
         public int getSeaLevel() {
            return real.getSeaLevel();
         }

         @Override
         public int getMinY() {
            return real.getMinY();
         }

         @Override
         public int getBaseHeight(int x, int z, net.minecraft.world.level.levelgen.Heightmap.Types type, LevelHeightAccessor heightAccessor, net.minecraft.world.level.levelgen.RandomState randomState) {
            throw new UnsupportedOperationException();
         }

         @Override
         public NoiseColumn getBaseColumn(int x, int z, LevelHeightAccessor heightAccessor, net.minecraft.world.level.levelgen.RandomState randomState) {
            throw new UnsupportedOperationException();
         }

         @Override
         public void addDebugScreenInfo(List<String> result, net.minecraft.world.level.levelgen.RandomState randomState, BlockPos feetPos,
            net.minecraft.world.level.levelgen.densityfunction.SamplerContext samplerContext) {
         }
      };
   }

   static String biomeName(Holder<Biome> h) {
      return h.unwrapKey().orElseThrow().identifier().toString();
   }

   static void dumpFeatures(PrintWriter out, String gen, ChunkGenerator generator) {
      List<Holder<Biome>> biomes = List.copyOf(generator.getBiomeSource().possibleBiomes());
      List<FeatureSorter.StepFeatureData> steps = FeatureSorter.buildFeaturesPerStep(biomes, b -> generator.getBiomeGenerationSettings(b).features(), true);
      for (int i = 0; i < steps.size(); i++) {
         List<String> names = new ArrayList<>();
         for (PlacedFeature f : steps.get(i).features()) {
            names.add(name(f));
         }
         out.println("FEATURES " + gen + " " + i + " " + String.join(",", names));
      }
   }

   static ProtoChunk chunk(int cx, int cz, List<Holder<Biome>> subset, int salt) {
      ProtoChunk chunk = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, LevelHeightAccessor.create(minY, height), PalettedContainerFactory.create(access), null);
      LevelChunkSection[] sections = chunk.getSections();
      for (int i = 0; i < sections.length; i++) {
         final int section = i;
         sections[i].fillBiomesFromNoise((x, y, z) -> subset.get(Math.floorMod((x & 3) + (y & 3) * 3 + (z & 3) * 5 + section + salt, subset.size())), 0, 0, 0);
      }
      return chunk;
   }

   static void decorate(PrintWriter out, String gen, ChunkGenerator real, ChunkGenerator stand, long seed, int cx, int cz, List<Holder<Biome>> subset) throws Exception {
      Map<Long, ChunkAccess> chunks = new java.util.HashMap<>();
      Set<String> present = new TreeSet<>();
      for (int dx = -1; dx <= 1; dx++) {
         for (int dz = -1; dz <= 1; dz++) {
            ProtoChunk c = chunk(cx + dx, cz + dz, subset, dx + dz * 2 + 7);
            chunks.put(ChunkPos.pack(cx + dx, cz + dz), c);
            for (LevelChunkSection section : c.getSections()) {
               section.getBiomes().getAll(h -> present.add(biomeName(h)));
            }
         }
      }
      out.println("CASE " + gen + " " + seed + " " + cx + " " + cz + " " + String.join(",", present));
      WorldgenRandom dr = new WorldgenRandom(new XoroshiroRandomSource(0));
      out.println("DECSEED " + dr.setDecorationSeed(seed, cx << 4, cz << 4));
      WorldGenLevel level = (WorldGenLevel) Proxy.newProxyInstance(DecorationOracle.class.getClassLoader(), new Class<?>[]{WorldGenLevel.class}, (proxy, method, args) -> {
         switch (method.getName()) {
            case "getSeed": return seed;
            case "getMinSectionY": return minY >> 4;
            case "getMinY": return minY;
            case "getHeight": return height;
            case "getMaxY": return minY + height - 1;
            case "registryAccess": return access;
            case "setCurrentlyGenerating": return null;
            case "getChunk":
               if (args.length == 2) {
                  return chunks.get(ChunkPos.pack((Integer) args[0], (Integer) args[1]));
               }
            default: throw new UnsupportedOperationException(method.toString());
         }
      });
      StructureManager manager = (StructureManager) unsafe().allocateInstance(StructureManager.class);
      Field f = StructureManager.class.getDeclaredField("worldOptions");
      f.setAccessible(true);
      f.set(manager, new WorldOptions(seed, false, false));
      placed.clear();
      stand.applyBiomeDecoration(level, chunks.get(ChunkPos.pack(cx, cz)), manager);
      for (String p : placed) {
         out.println("PLACE " + p);
      }
   }

   static sun.misc.Unsafe unsafe() throws Exception {
      Field f = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      f.setAccessible(true);
      return (sun.misc.Unsafe) f.get(null);
   }

   static void generator(PrintWriter out, String gen, String flatJson, ChunkGenerator real, int dimMinY, int dimHeight) throws Exception {
      out.println("GEN " + gen + " " + (flatJson == null ? "-" : flatJson));
      dumpFeatures(out, gen, real);
      minY = dimMinY;
      height = dimHeight;
      ChunkGenerator stand = standIn(real);
      List<Holder<Biome>> possible = List.copyOf(real.getBiomeSource().possibleBiomes());
      Random rnd = new Random(gen.hashCode());
      List<List<Holder<Biome>>> subsets = new ArrayList<>();
      for (Holder<Biome> b : possible) {
         subsets.add(List.of(b));
      }
      for (int i = 0; i < 6 && possible.size() > 1; i++) {
         List<Holder<Biome>> s = new ArrayList<>();
         int n = 2 + rnd.nextInt(Math.min(6, possible.size() - 1));
         for (int j = 0; j < n; j++) {
            Holder<Biome> b = possible.get(rnd.nextInt(possible.size()));
            if (!s.contains(b)) {
               s.add(b);
            }
         }
         subsets.add(s);
      }
      long[] seeds = {0L, 123456789L, -4172144997902289642L};
      int[][] chunkPositions = {{0, 0}, {-3, 7}};
      int k = 0;
      for (List<Holder<Biome>> subset : subsets) {
         // overworld has 50+ biomes: 2 seed/chunk combinations per subset keep the fixture small
         for (int c = 0; c < seeds.length * chunkPositions.length; c++) {
            if (possible.size() > 8 && (c + k) % 3 != 0) {
               continue;
            }
            decorate(out, gen, real, stand, seeds[c % seeds.length], chunkPositions[c / seeds.length][0], chunkPositions[c / seeds.length][1], subset);
         }
         k++;
      }
   }

   public static void main(String[] args) throws Exception {
      access = extractor.Setup.bootstrap();
      ops = RegistryOps.create(JsonOps.INSTANCE, access);
      biomeRegistry = access.lookupOrThrow(Registries.BIOME);
      placedRegistry = access.lookupOrThrow(Registries.PLACED_FEATURE);
      var dims = access.lookupOrThrow(Registries.WORLD_PRESET).getValue(Identifier.withDefaultNamespace("normal")).createWorldDimensions().dimensions();
      try (PrintWriter out = new PrintWriter(args[0])) {
         for (String dim : new String[]{"overworld", "the_nether", "the_end"}) {
            LevelStem stem = dims.entrySet().stream().filter(e -> e.getKey().identifier().getPath().equals(dim)).findFirst().orElseThrow().getValue();
            int[] range = dim.equals("overworld") ? new int[]{-64, 384} : new int[]{0, 256};
            generator(out, dim, null, stem.generator(), range[0], range[1]);
         }
         for (int i = 0; i < FLATS.length; i++) {
            FlatLevelGeneratorSettings settings = FlatLevelGeneratorSettings.CODEC.parse(ops, JsonParser.parseString(FLATS[i])).getOrThrow();
            generator(out, "flat" + i, FLATS[i], new FlatLevelSource(settings), -64, 384);
         }
      }
   }
}
