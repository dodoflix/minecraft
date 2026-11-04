import java.io.PrintWriter;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Predicate;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderGetter;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.NoiseColumn;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.BiomeResolver;
import net.minecraft.world.level.biome.BiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.biome.TheEndBiomeSource;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Aquifer;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.NoiseChunk;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.NoiseSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.blending.Blender;
import net.minecraft.world.level.levelgen.densityfunction.DensitySampler;
import net.minecraft.world.level.levelgen.densityfunction.DensityVolume;
import net.minecraft.world.level.levelgen.densityfunction.ScopedDensityBuffer;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.synth.NormalNoise;

/**
 * Golden generator for the mc-worldgen terrain pipeline (NoiseBasedChunkGenerator, Aquifer, NoiseChunk).
 * Usage: scripts/vanilla-java.sh -cp ... is wrapped: java -cp tools/extractor/build:$(scripts/vanilla-cp.sh) TerrainOracle.java > tests/fixtures/terrain_golden.txt
 * (see crates/mc-worldgen/tests/oracle/run_terrain_oracle.sh).
 *
 * Hashes are FNV-1a 64 over little-endian u32 words (block state ids) / UTF-8 bytes + '\n' (biome keys) / i64 words.
 * Lines (all start with a tag; setting seed cx cz identify the case):
 *   D  setting seed cx cz  <section hashes...>          blocks after doFill (one hash per section, bottom up)
 *   DH setting seed cx cz type hash                     heightmap raw data after doFill
 *   DP setting seed cx cz hash                          post-processing marks after doFill
 *   B  setting seed cx cz  hash                         biomes after createBiomes (all quart cells, section by section)
 *   F  setting seed cx cz  <section hashes...>          blocks after the full buildTerrain
 *   FH setting seed cx cz type hash                     heightmaps (primed ones) after the full buildTerrain
 *   FP setting seed cx cz hash                          post-processing marks after the full buildTerrain
 *   A  setting seed cx cz count hash                    Aquifer.computeSubstance sequence digest (+ scheduled flag)
 *   H  setting seed x z type height                     getBaseHeight
 *   K  setting seed x z hash                            getBaseColumn
 *   O  setting seed cx cz x z                           getOrigin (only per setting/seed, cx cz = 0)  chunkX chunkZ
 */
public class TerrainOracle {
   static final String[] SETTINGS = {"overworld", "large_biomes", "amplified", "nether", "end", "caves", "floating_islands"};
   static final long[] SEEDS = {0L, 12345L, -4172144997902289642L};
   static final int[][] CHUNKS = {
      {0, 0}, {-1, -1}, {3, 7}, {-15, 22}, {40, -33}, {100, 100}, {-200, 150}, {1000, -1000}, {5, -5}, {-64, 64}, {250, 13}, {-7, -90}
   };
   static final int[][] AQUIFER_CHUNKS = {{0, 0}, {-15, 22}, {40, -33}, {-7, -90}};
   static final int[][] COLUMNS = {
      {0, 0}, {-1, -1}, {17, 33}, {-250, 91}, {1234, -987}, {5, 5}, {-64, 128}, {300, -300}, {7, -15}, {2048, 2048}
   };

   static long fnv(long h, int word) {
      for (int i = 0; i < 4; i++) {
         h ^= (word >>> (8 * i)) & 0xFF;
         h *= 0x100000001b3L;
      }
      return h;
   }

   static long fnv(long h, long word) {
      return fnv(fnv(h, (int)word), (int)(word >>> 32));
   }

   static long fnvBytes(long h, byte[] bytes) {
      for (byte b : bytes) {
         h ^= b & 0xFF;
         h *= 0x100000001b3L;
      }
      return h;
   }

   static final long FNV_INIT = 0xcbf29ce484222325L;

   static String hex(long h) {
      return String.format("%016x", h);
   }

   static int stateId(BlockState state) {
      return Block.BLOCK_STATE_REGISTRY.getId(state);
   }

   static String sectionHashes(ChunkAccess chunk) {
      StringBuilder sb = new StringBuilder();
      int minY = chunk.getMinY();
      int count = chunk.getSectionsCount();
      BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
      for (int s = 0; s < count; s++) {
         long h = FNV_INIT;
         int baseY = minY + s * 16;
         for (int y = 0; y < 16; y++)
            for (int z = 0; z < 16; z++)
               for (int x = 0; x < 16; x++) {
                  p.set(chunk.getPos().getMinBlockX() + x, baseY + y, chunk.getPos().getMinBlockZ() + z);
                  h = fnv(h, stateId(chunk.getBlockState(p)));
               }
         sb.append(' ').append(hex(h));
      }
      return sb.toString();
   }

   static long biomeHash(ChunkAccess chunk) {
      long h = FNV_INIT;
      int qMinY = chunk.getMinY() >> 2;
      int qMaxY = qMinY + (chunk.getHeight() >> 2);
      for (int qy = qMinY; qy < qMaxY; qy++)
         for (int qz = 0; qz < 4; qz++)
            for (int qx = 0; qx < 4; qx++) {
               Holder<Biome> b = chunk.getNoiseBiome(chunk.getPos().getMinBlockX() / 4 + qx, qy, chunk.getPos().getMinBlockZ() / 4 + qz);
               h = fnvBytes(h, (b.unwrapKey().get().identifier().toString() + "\n").getBytes(java.nio.charset.StandardCharsets.UTF_8));
            }
      return h;
   }

   static long heightmapHash(Heightmap hm) {
      long h = FNV_INIT;
      for (long l : hm.getRawData()) h = fnv(h, l);
      return h;
   }

   static void printHeightmaps(PrintWriter out, String tag, String prefix, ChunkAccess chunk) {
      for (Heightmap.Types t : Heightmap.Types.values()) {
         if (chunk.hasPrimedHeightmap(t)) {
            out.println(tag + " " + prefix + " " + t.name() + " " + hex(heightmapHash(chunk.getOrCreateHeightmapUnprimed(t))));
         }
      }
   }

   static long postProcessingHash(ChunkAccess chunk) {
      long h = FNV_INIT;
      it.unimi.dsi.fastutil.shorts.ShortList[] lists = chunk.getPostProcessing();
      for (int i = 0; i < lists.length; i++) {
         h = fnv(h, -1 - i);
         if (lists[i] != null) {
            for (short s : lists[i]) h = fnv(h, (int)s);
         }
      }
      return h;
   }

   static class StubStructureManager extends StructureManager {
      StubStructureManager() {
         super(null, null, null);
      }

      @Override
      public List<StructureStart> startsForStructure(int sectionX, int sectionZ, Predicate<Structure> matcher) {
         return List.of();
      }

      @Override
      public StructureManager forWorldGenRegion(net.minecraft.server.level.WorldGenRegion region) {
         return this;
      }
   }

   public static void main(String[] args) throws Exception {
      RegistryAccess.Frozen access = extractor.Setup.bootstrap();
      HolderGetter<NormalNoise> noises = access.lookupOrThrow(Registries.NOISE);
      var settingsLookup = access.lookupOrThrow(Registries.NOISE_SETTINGS);
      HolderGetter<Biome> biomeGetter = access.lookupOrThrow(Registries.BIOME);
      var presetLookup = access.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST);
      PalettedContainerFactory factory = PalettedContainerFactory.create(access);
      PrintWriter out = new PrintWriter(System.out);
      StructureManager sm = new StubStructureManager();

      Method createNoiseChunk = NoiseBasedChunkGenerator.class.getDeclaredMethod(
         "createNoiseChunk", ChunkAccess.class, StructureManager.class, Blender.class, RandomState.class, NoiseSettings.class
      );
      createNoiseChunk.setAccessible(true);
      Method doFill = NoiseBasedChunkGenerator.class.getDeclaredMethod("doFill", NoiseChunk.class, ChunkAccess.class);
      doFill.setAccessible(true);

      for (String name : SETTINGS) {
         Holder<NoiseGeneratorSettings> holder = settingsLookup.getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace(name)));
         NoiseGeneratorSettings settings = holder.value();
         boolean tall = name.equals("overworld") || name.equals("large_biomes") || name.equals("amplified") || name.equals("caves");
         LevelHeightAccessor height = tall ? LevelHeightAccessor.create(-64, 384) : LevelHeightAccessor.create(0, 256);
         BiomeSource source;
         if (name.equals("nether")) {
            source = MultiNoiseBiomeSource.createFromPreset(presetLookup.getOrThrow(MultiNoiseBiomeSourceParameterLists.NETHER));
         } else if (name.equals("end") || name.equals("floating_islands")) {
            source = TheEndBiomeSource.create(biomeGetter);
         } else {
            source = MultiNoiseBiomeSource.createFromPreset(presetLookup.getOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD));
         }
         NoiseBasedChunkGenerator gen = new NoiseBasedChunkGenerator(source, holder);
         for (long seed : SEEDS) {
            RandomState rs = RandomState.create(noises, seed, settings);
            String key = name + " " + seed;
            BiomeResolver uncached = source.createUncachedResolver(rs);
            BiomeManager biomeManager = new BiomeManager(uncached::getNoiseBiome, BiomeManager.obfuscateSeed(seed));
            ChunkPos origin = gen.getOrigin(rs);
            out.println("O " + key + " 0 0 " + origin.x() + " " + origin.z());

            for (int[] c : CHUNKS) {
               ChunkPos pos = new ChunkPos(c[0], c[1]);
               String id = key + " " + c[0] + " " + c[1];

               // biomes
               ProtoChunk biomeChunk = new ProtoChunk(pos, UpgradeData.EMPTY, height, factory, null);
               gen.createBiomes(rs, Blender.empty(), sm, biomeChunk).join();
               out.println("B " + id + " " + hex(biomeHash(biomeChunk)));

               // doFill only
               ProtoChunk fill = new ProtoChunk(pos, UpgradeData.EMPTY, height, factory, null);
               fill.setPersistedStatus(ChunkStatus.BIOMES);
               NoiseSettings ns = settings.noiseSettings().clampToHeightAccessor(fill.getHeightAccessorForGeneration());
               try (NoiseChunk nc = (NoiseChunk)createNoiseChunk.invoke(gen, fill, sm, Blender.empty(), rs, ns)) {
                  doFill.invoke(gen, nc, fill);
               }
               out.println("D " + id + sectionHashes(fill));
               printHeightmaps(out, "DH", id, fill);
               out.println("DP " + id + " " + hex(postProcessingHash(fill)));

               // full pipeline
               Set<Holder<Biome>> possible = new HashSet<>();
               ProtoChunk full = null;
               for (int dz = -1; dz <= 1; dz++) {
                  for (int dx = -1; dx <= 1; dx++) {
                     ProtoChunk n = (dx == 0 && dz == 0) ? biomeChunk : new ProtoChunk(new ChunkPos(c[0] + dx, c[1] + dz), UpgradeData.EMPTY, height, factory, null);
                     if (n != biomeChunk) gen.createBiomes(rs, Blender.empty(), sm, n).join();
                     n.collectBiomesInPalette(possible);
                  }
               }
               full = biomeChunk;
               full.setPersistedStatus(ChunkStatus.BIOMES);
               gen.buildTerrain(full, Blender.empty(), rs, sm, biomeManager, null, possible).join();
               out.println("F " + id + sectionHashes(full));
               printHeightmaps(out, "FH", id, full);
               out.println("FP " + id + " " + hex(postProcessingHash(full)));
            }

            // aquifer digest
            for (int[] c : AQUIFER_CHUNKS) {
               ChunkPos pos = new ChunkPos(c[0], c[1]);
               ProtoChunk chunk = new ProtoChunk(pos, UpgradeData.EMPTY, height, factory, null);
               NoiseSettings ns = settings.noiseSettings().clampToHeightAccessor(chunk.getHeightAccessorForGeneration());
               try (NoiseChunk nc = (NoiseChunk)createNoiseChunk.invoke(gen, chunk, sm, Blender.empty(), rs, ns)) {
                  Aquifer aq = nc.aquifer();
                  long h = FNV_INIT;
                  int n = 0;
                  double[] densities = {0.0, -0.1, 0.05, -2.0};
                  for (int dx : new int[]{0, 5, 11, 15}) {
                     for (int dz : new int[]{0, 6, 15}) {
                        for (int y = ns.minY() + ns.height() - 1; y >= ns.minY(); y -= 5) {
                           double d = densities[Math.floorMod(n, densities.length)];
                           BlockState s = aq.computeSubstance(pos.getMinBlockX() + dx, y, pos.getMinBlockZ() + dz, d);
                           h = fnv(h, s == null ? -1 : stateId(s));
                           h = fnv(h, aq.shouldScheduleFluidUpdate() ? 1 : 0);
                           n++;
                        }
                     }
                  }
                  out.println("A " + key + " " + c[0] + " " + c[1] + " " + n + " " + hex(h));
               }
            }

            // base height / column
            for (int[] c : COLUMNS) {
               for (Heightmap.Types t : Heightmap.Types.values()) {
                  out.println("H " + key + " " + c[0] + " " + c[1] + " " + t.name() + " " + gen.getBaseHeight(c[0], c[1], t, height, rs));
               }
               NoiseColumn column = gen.getBaseColumn(c[0], c[1], height, rs);
               long h = FNV_INIT;
               int minY = height.getMinY();
               for (int y = minY; y < minY + height.getHeight(); y++) h = fnv(h, stateId(column.getBlock(y)));
               out.println("K " + key + " " + c[0] + " " + c[1] + " " + hex(h));
            }
         }
      }
      out.flush();
   }
}
