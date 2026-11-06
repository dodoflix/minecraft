import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.core.HolderGetter;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.densityfunction.DensityBuffer;
import net.minecraft.world.level.levelgen.densityfunction.DensityFunction;
import net.minecraft.world.level.levelgen.densityfunction.DensitySampler;
import net.minecraft.world.level.levelgen.densityfunction.DensityVolume;
import net.minecraft.world.level.levelgen.densityfunction.SamplerContext;
import net.minecraft.world.level.levelgen.synth.NormalNoise;

/**
 * Golden generator: EVERY registered density function (not just router entries) per setting/seed.
 * Same line format as RandomStateOracle; function names are "df:<identifier>".
 * Usage: scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/density_functions.txt crates/mc-worldgen/tests/oracle/DensityFunctionsOracle.java
 * Output: crates/mc-worldgen/tests/fixtures/density_functions.txt (the wrapper captures stdout)
 *
 * Lines:  P setting seed fn x y z bits        point sample (EMPTY_UNCACHED)
 *         C setting seed fn x y z bits        point sample with caches enabled (one context for the whole grid)
 *         V setting seed fn vol n bits...     volume sample (EMPTY_UNCACHED); vol = index into VOLUMES
 */
public class DensityFunctionsOracle {
   static final String[] SETTINGS = {"overworld", "nether", "end"};
   static final long[] SEEDS = {12345L, -4172144997902289642L};
   static final int[] XS = {-333, 5, 64};
   static final int[] YS = {-60, 40, 200};
   static final int[] ZS = {3, 250};
   // sizeX,sizeY,sizeZ,minX,minY,minZ,stepX,stepY,stepZ
   static final int[][] VOLUMES = {
      {5, 4, 5, -17, -12, 3, 1, 1, 1},
      {4, 6, 4, -16, -8, 32, 4, 8, 4},
      {1, 1, 1, 5, 37, -19, 1, 1, 1},
      {3, 5, 2, 2, 17, -6, 2, 3, 5}
   };

   static String bits(float f) {
      return String.format("%08x", Float.floatToRawIntBits(f));
   }

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      HolderLookup.Provider provider = VanillaRegistries.createWorldLookup();
      HolderGetter<NormalNoise> noises = provider.lookupOrThrow(Registries.NOISE);
      HolderLookup<NoiseGeneratorSettings> settingsLookup = provider.lookupOrThrow(Registries.NOISE_SETTINGS);
      PrintWriter out = new PrintWriter(System.out);
      for (String name : SETTINGS) {
         NoiseGeneratorSettings settings = settingsLookup.getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace(name))).value();
         Map<String, DensityFunction> fns = new LinkedHashMap<>();
         provider.lookupOrThrow(Registries.DENSITY_FUNCTION).listElements().sorted(java.util.Comparator.comparing(h -> h.key().identifier().toString())).forEach(h -> fns.put("df:" + h.key().identifier(), h.value()));
         for (long seed : SEEDS) {
            RandomState rs = RandomState.create(noises, seed, settings);
            SamplerContext cached = SamplerContext.builder().enableCaches().build();
            for (Map.Entry<String, DensityFunction> e : fns.entrySet()) {
               DensitySampler sampler = rs.getSampler(e.getValue());
               for (int x : XS) for (int y : YS) for (int z : ZS) {
                  out.println("P " + name + " " + seed + " " + e.getKey() + " " + x + " " + y + " " + z + " " + bits(rs.sampleBlockValueUncached(e.getValue(), x, y, z)));
                  out.println("C " + name + " " + seed + " " + e.getKey() + " " + x + " " + y + " " + z + " " + bits(sampler.sampleValue(cached, x, y, z)));
               }
               for (int v = 0; v < VOLUMES.length; v++) {
                  int[] d = VOLUMES[v];
                  DensityVolume vol = new DensityVolume(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8]);
                  DensityBuffer buf = DensityBuffer.createUnpooled(vol.size());
                  sampler.sampleVolume(SamplerContext.EMPTY_UNCACHED, buf, vol);
                  StringBuilder sb = new StringBuilder("V " + name + " " + seed + " " + e.getKey() + " " + v + " " + vol.size());
                  for (int i = 0; i < vol.size(); i++) sb.append(' ').append(bits(buf.get(i)));
                  out.println(sb);
               }
            }
         }
      }
      out.flush();
   }
}
