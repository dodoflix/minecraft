import net.minecraft.util.Interval;
import net.minecraft.world.level.levelgen.densityfunction.*;
import net.minecraft.world.level.levelgen.densityfunction.generator.ConstantFunction;
import net.minecraft.world.level.levelgen.densityfunction.op.*;
import com.mojang.serialization.MapCodec;

/** Golden values for the op/ samplers (InterpolatedFunction, RangeChoiceFunction). Run: scripts/vanilla-java.sh OpsOracle.java | sed "s/.*\[STDOUT\]: //" | grep -E "^(point|volume|rangechoice)" > tests/fixtures/ops.txt */
public class OpsOracle {
    static float f(int x, int y, int z) {
        return x * 0.37f + y * y * 0.011f - z * 0.21f + (x * z) * 0.003f;
    }

    /** A function whose compiled sampler evaluates f(x,y,z) and records nothing. */
    static final class Fn implements DensityFunction {
        public DensitySampler compileSampler(CompileContext c) {
            return new DensitySampler() {
                public void sampleVolume(SamplerContext ctx, DensityBuffer out, DensityVolume v) {
                    DensitySampler.sampleVolumeNaive(ctx, out, v, this);
                }
                public float sampleValue(SamplerContext ctx, int x, int y, int z) {
                    return f(x, y, z);
                }
            };
        }
        public DensityFunction rewriteChildren(DfRewriteRule r) { return this; }
        public Interval range() { return Interval.INFINITE; }
        public int domainAxes() { return 7; }
        public MapCodec<? extends DensityFunction> codec() { return null; }
    }

    static void volume(String name, DensitySampler s, DensityVolume v) {
        SamplerContext ctx = SamplerContext.EMPTY_UNCACHED;
        try (ScopedDensityBuffer b = ctx.acquireBuffer(v)) {
            s.sampleVolume(ctx, b, v);
            for (int i = 0; i < v.size(); i++) {
                System.out.println("volume " + name + " " + i + " " + Integer.toHexString(Float.floatToRawIntBits(b.get(i))));
            }
        }
    }

    public static void main(String[] a) {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        DensitySampler s = new InterpolatedFunction(new Fn(), 4, 8).compileSampler(null);
        SamplerContext ctx = SamplerContext.EMPTY_UNCACHED;
        int[][] pts = {{5, 3, 7}, {-3, -5, 2}, {4, 8, 12}, {0, 1, 0}, {-1, 15, -2}, {13, 9, 6}};
        for (int[] p : pts) {
            System.out.println("point " + p[0] + " " + p[1] + " " + p[2] + " "
                + Integer.toHexString(Float.floatToRawIntBits(s.sampleValue(ctx, p[0], p[1], p[2]))));
        }
        volume("block", s, new DensityVolume(6, 7, 5, -3, -2, 1));
        volume("nonblock", s, new DensityVolume(3, 3, 3, -5, -3, 2, 2, 5, 3));
        volume("aligned", s, new DensityVolume(3, 2, 3, 8, 16, -4, 4, 8, 4));
        volume("flat", s, new DensityVolume(1, 9, 1, 4, 0, 4));

        DensitySampler rc = new RangeChoiceFunction(new ConstantFunction(0.5f), 0.0f, 1.0f, new ConstantFunction(5.0f), new ConstantFunction(7.0f)).compileSampler(null);
        DensitySampler rc2 = new RangeChoiceFunction(new ConstantFunction(2.0f), 0.0f, 1.0f, new ConstantFunction(5.0f), new ConstantFunction(7.0f)).compileSampler(null);
        DensitySampler rc3 = new RangeChoiceFunction(new ConstantFunction(0.5f), 0.0f, 1.0f, new Fn(), new ConstantFunction(7.0f)).compileSampler(null);
        System.out.println("rangechoice const in " + rc.sampleValue(ctx, 0, 0, 0));
        System.out.println("rangechoice const out " + rc2.sampleValue(ctx, 0, 0, 0));
        System.out.println("rangechoice mixed in " + rc3.sampleValue(ctx, 1, 2, 3));
    }
}
