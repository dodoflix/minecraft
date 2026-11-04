import com.mojang.datafixers.util.Pair;
import java.io.FileOutputStream;
import java.io.PrintWriter;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import java.util.function.Predicate;
import java.util.zip.GZIPOutputStream;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderGetter;
import net.minecraft.core.HolderSet;
import net.minecraft.core.QuartPos;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.BiomeResolver;
import net.minecraft.world.level.biome.BiomeSource;
import net.minecraft.world.level.biome.CheckerboardColumnBiomeSource;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.biome.FixedBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.biome.TheEndBiomeSource;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.NoiseSpawnFinder;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.densityfunction.SamplerContext;
import net.minecraft.world.level.levelgen.synth.NormalNoise;

/**
 * Golden generator for mc-worldgen biome sampling tests: climate sampling, biome resolvers (point + chunk volume),
 * BiomeManager, biome searches and the spawn finder, for every real biome source type.
 * Usage (from the repo root):
 *   java -cp tools/extractor/build:$(scripts/vanilla-cp.sh) crates/mc-worldgen/tests/oracle/BiomeSamplingOracle.java crates/mc-worldgen/tests/fixtures/biome_sampling.txt.gz
 *
 * Lines: P source id...                         possibleBiomes() in order
 *        T source seed qx qy qz t h c e d w biome   Climate.Sampler.sample + resolver biome (climate sources)
 *        K source seed qx qy qz biome           resolver biome (non-climate sources)
 *        M source seed x y z biome              BiomeManager.getBiome
 *        V source seed cx cz minQuartY sizeY chars   createResolverForChunk biomes (x, z, y order, char = '0' + id)
 *        H source seed variant x y z biome|null  findBiomeHorizontal
 *        D source seed x y z biome|null          findClosestBiome3d
 *        S settings seed x z                      NoiseSpawnFinder.findSpawnPosition
 */
public class BiomeSamplingOracle {
   static final long[] SEEDS = {0L, 12345L, -4172144997902289642L, Long.MAX_VALUE, Long.MIN_VALUE};
   static final int[] QXS = {-301, -40, 0, 17, 205};
   static final int[] QZS = {-120, -7, 0, 3, 333};
   static final int[] QYS = {-16, -3, 0, 4, 16, 48, 79};
   static final int[][] CHUNKS = {{0, 0}, {-5, 7}, {31, -20}};

   static Registry<Biome> biomes;
   static PrintWriter out;

   static Holder<Biome> biome(String name) {
      return biomes.getOrThrow(ResourceKey.create(Registries.BIOME, Identifier.withDefaultNamespace(name)));
   }

   static int id(Holder<Biome> h) {
      return biomes.getId(h.value());
   }

   static LevelReader level(int minY, int height) {
      return (LevelReader) Proxy.newProxyInstance(LevelReader.class.getClassLoader(), new Class[]{LevelReader.class}, (p, m, a) -> {
         return switch (m.getName()) {
            case "getMinY" -> minY;
            case "getMaxY" -> minY + height - 1;
            case "getHeight" -> height;
            default -> throw new UnsupportedOperationException(m.getName());
         };
      });
   }

   static String res(Pair<BlockPos, Holder<Biome>> p) {
      return p == null ? "null" : p.getFirst().getX() + " " + p.getFirst().getY() + " " + p.getFirst().getZ() + " " + id(p.getSecond());
   }

   public static void main(String[] args) throws Exception {
      RegistryAccess.Frozen reg = extractor.Setup.bootstrap();
      biomes = reg.lookupOrThrow(Registries.BIOME);
      HolderGetter<NormalNoise> noises = reg.lookupOrThrow(Registries.NOISE);
      var settingsLookup = reg.lookupOrThrow(Registries.NOISE_SETTINGS);
      var lists = reg.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST);
      out = new PrintWriter(new GZIPOutputStream(new FileOutputStream(args[0])));

      BiomeSource overworld = MultiNoiseBiomeSource.createFromPreset(lists.getOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD));
      BiomeSource nether = MultiNoiseBiomeSource.createFromPreset(lists.getOrThrow(MultiNoiseBiomeSourceParameterLists.NETHER));
      BiomeSource end = TheEndBiomeSource.create(biomes);
      BiomeSource fixed = new FixedBiomeSource(biome("plains"));
      BiomeSource checker = new CheckerboardColumnBiomeSource(
         HolderSet.direct(List.of(biome("plains"), biome("forest"), biome("desert"), biome("ocean"), biome("jungle"), biome("forest"))), 3);
      BiomeSource checkerDefault = new CheckerboardColumnBiomeSource(HolderSet.direct(List.of(biome("badlands"), biome("swamp"), biome("taiga"))), 2);

      // name, source, noise settings, climate-sampled?, min quart y, quart size y
      Object[][] cases = {
         {"overworld", overworld, "overworld", true, -16, 96},
         {"large_biomes", overworld, "large_biomes", true, -16, 96},
         {"amplified", overworld, "amplified", true, -16, 96},
         {"nether", nether, "nether", true, 0, 32},
         {"end", end, "end", false, 0, 32},
         {"fixed", fixed, "overworld", false, -16, 96},
         {"checker", checker, "overworld", false, -16, 96},
         {"checker_default", checkerDefault, "overworld", false, -16, 96},
      };
      for (Object[] c : cases) {
         String name = (String) c[0];
         BiomeSource source = (BiomeSource) c[1];
         boolean climate = (Boolean) c[3];
         int minQy = (Integer) c[4];
         int sizeY = (Integer) c[5];
         NoiseGeneratorSettings settings = settingsLookup.getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace((String) c[2]))).value();
         StringBuilder possible = new StringBuilder();
         for (Holder<Biome> b : source.possibleBiomes()) possible.append(' ').append(id(b));
         out.println("P " + name + possible);
         for (long seed : SEEDS) {
            RandomState rs = RandomState.create(noises, seed, settings);
            Climate.Sampler sampler = rs.createClimateSampler(SamplerContext.EMPTY_UNCACHED);
            BiomeResolver resolver = source.createResolver(sampler);
            for (int qx : QXS) for (int qz : QZS) for (int qy : QYS) {
               int b = id(resolver.getNoiseBiome(qx, qy, qz));
               if (climate) {
                  Climate.TargetPoint t = sampler.sample(qx, qy, qz);
                  out.println("T " + name + " " + seed + " " + qx + " " + qy + " " + qz + " " + t.temperature() + " " + t.humidity() + " "
                     + t.continentalness() + " " + t.erosion() + " " + t.depth() + " " + t.weirdness() + " " + b);
               } else {
                  out.println("K " + name + " " + seed + " " + qx + " " + qy + " " + qz + " " + b);
               }
            }
            // BiomeManager (fiddled) at random block positions
            BiomeManager manager = new BiomeManager(source.createUncachedResolver(rs), BiomeManager.obfuscateSeed(seed));
            Random r = new Random(seed ^ 0x5DEECE66DL);
            for (int i = 0; i < 250; i++) {
               int x = r.nextInt(4000) - 2000;
               int y = r.nextInt(400) - 80;
               int z = r.nextInt(4000) - 2000;
               out.println("M " + name + " " + seed + " " + x + " " + y + " " + z + " " + id(manager.getBiome(new BlockPos(x, y, z))));
            }
            // chunk volumes (caching context, as ChunkGenerator.doCreateBiomes)
            for (int[] ch : CHUNKS) {
               Climate.Sampler cs = rs.createClimateSampler(SamplerContext.builder().enableCaches().build());
               int minQx = QuartPos.fromBlock(ch[0] * 16);
               int minQz = QuartPos.fromBlock(ch[1] * 16);
               BiomeResolver cr = source.createResolverForChunk(cs, minQx, minQy, minQz, 4, sizeY, 4);
               StringBuilder sb = new StringBuilder();
               for (int x = 0; x < 4; x++) for (int z = 0; z < 4; z++) for (int y = 0; y < sizeY; y++) {
                  sb.append((char) ('0' + id(cr.getNoiseBiome(minQx + x, minQy + y, minQz + z))));
               }
               out.println("V " + name + " " + seed + " " + ch[0] + " " + ch[1] + " " + minQy + " " + sizeY + " " + sb);
            }
            // biome searches (a rare-ish predicate: ids congruent 1 mod 4)
            if (seed == 12345L || seed == Long.MIN_VALUE) {
               Predicate<Holder<Biome>> allowed = h -> id(h) % 4 == 1;
               Predicate<Holder<Biome>> any = h -> true;
               int[][] origins = {{0, 64, 0}, {-1234, 90, 777}};
               for (int[] o : origins) {
                  out.println("H " + name + " " + seed + " r400 " + o[0] + " " + o[1] + " " + o[2] + " "
                     + res(source.findBiomeHorizontal(o[0], o[1], o[2], 400, allowed, new LegacyRandomSource(seed), rs)));
                  out.println("H " + name + " " + seed + " r400any " + o[0] + " " + o[1] + " " + o[2] + " "
                     + res(source.findBiomeHorizontal(o[0], o[1], o[2], 400, any, new LegacyRandomSource(seed), rs)));
                  out.println("H " + name + " " + seed + " closest " + o[0] + " " + o[1] + " " + o[2] + " "
                     + res(source.findBiomeHorizontal(o[0], o[1], o[2], 2000, 8, allowed, new LegacyRandomSource(seed), true, rs)));
                  out.println("H " + name + " " + seed + " skip " + o[0] + " " + o[1] + " " + o[2] + " "
                     + res(source.findBiomeHorizontal(o[0], o[1], o[2], 1200, 4, allowed, new LegacyRandomSource(seed + 1), false, rs)));
                  out.println("D " + name + " " + seed + " " + o[0] + " " + o[1] + " " + o[2] + " "
                     + res(source.findClosestBiome3d(new BlockPos(o[0], o[1], o[2]), 1600, 32, 64, allowed, rs, level(minQy * 4, sizeY * 4))));
                  out.println("D2 " + name + " " + seed + " " + o[0] + " " + o[1] + " " + o[2] + " "
                     + res(source.findClosestBiome3d(new BlockPos(o[0], o[1], o[2]), 600, 16, 24, allowed, rs, level(minQy * 4, sizeY * 4))));
               }
            }
         }
      }
      // spawn finder
      for (String name : new String[]{"overworld", "large_biomes", "amplified"}) {
         NoiseGeneratorSettings settings = settingsLookup.getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace(name))).value();
         List<Long> spawnSeeds = new ArrayList<>();
         for (long seed : SEEDS) spawnSeeds.add(seed);
         Random sr = new Random(77);
         for (int i = 0; i < 10; i++) spawnSeeds.add(sr.nextLong());
         for (long seed : spawnSeeds) {
            RandomState rs = RandomState.create(noises, seed, settings);
            BlockPos p = NoiseSpawnFinder.findSpawnPosition(settings.spawnTarget(), rs.samplersWithContext(SamplerContext.builder().enableCaches().build()));
            out.println("S " + name + " " + seed + " " + p.getX() + " " + p.getZ());
         }
      }
      out.close();
   }
}
