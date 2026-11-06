import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.densityfunction.DensityBuffer;
import net.minecraft.world.level.levelgen.densityfunction.DensityFunction;
import net.minecraft.world.level.levelgen.densityfunction.DensitySampler;
import net.minecraft.world.level.levelgen.densityfunction.DensityVolume;
import net.minecraft.world.level.levelgen.densityfunction.SamplerContext;
import net.minecraft.world.level.levelgen.densityfunction.generator.EndIslandFunction;

/**
 * Golden values for EndIslandFunction (RandomState.createEndIslandRandom() is `new LegacyRandomSource(seed)`).
 * Usage: scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/end_island.txt crates/mc-worldgen/tests/oracle/EndIslandOracle.java
 * Output: crates/mc-worldgen/tests/fixtures/end_island.txt (the wrapper captures stdout; main restores the real stdout so vanilla's [STDOUT] log wrapping is bypassed)
 * Lines: P seed blockX blockZ floatBits      point sample
 *        V seed floatBits...                  volume sample of DensityVolume(3, 2, 3, min=(-1030, 0, 1018), step=(8, 4, 8)) in buffer order
 */
public class EndIslandOracle {
   static final long[] SEEDS = {0L, 12345L, -4172144997902289642L};
   static final int[] COORDS = {-2600, -1500, -1100, -1030, -400, -9, 0, 7, 16, 500, 1018, 1100, 1300, 1700, 2100, 2600};

   public static void main(String[] args) {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      System.setOut(net.minecraft.server.Bootstrap.STDOUT);
      for (long seed : SEEDS) {
         DensityFunction.CompileContext context = new DensityFunction.CompileContext() {
            public net.minecraft.world.level.levelgen.synth.Noise createNoiseSampler(net.minecraft.core.Holder<net.minecraft.world.level.levelgen.synth.NormalNoise> p) {
               throw new UnsupportedOperationException();
            }

            public RandomSource createRandom(net.minecraft.resources.Identifier id) {
               throw new UnsupportedOperationException();
            }

            public RandomSource createEndIslandRandom() {
               return new LegacyRandomSource(seed);
            }
         };
         DensitySampler sampler = new EndIslandFunction().compileSampler(context);
         SamplerContext ctx = SamplerContext.EMPTY_UNCACHED;
         for (int x : COORDS) {
            for (int z : COORDS) {
               System.out.println("P " + seed + " " + x + " " + z + " " + Integer.toHexString(Float.floatToRawIntBits(sampler.sampleValue(ctx, x, 5, z))));
            }
         }
         DensityVolume volume = new DensityVolume(3, 2, 3, -1030, 0, 1018, 8, 4, 8);
         DensityBuffer out = DensityBuffer.createUnpooled(volume.size());
         sampler.sampleVolume(ctx, out, volume);
         StringBuilder line = new StringBuilder("V " + seed);
         for (int i = 0; i < out.size(); i++) {
            line.append(' ').append(Integer.toHexString(Float.floatToRawIntBits(out.get(i))));
         }
         System.out.println(line);
      }
   }
}
