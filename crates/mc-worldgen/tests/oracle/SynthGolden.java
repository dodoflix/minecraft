// Generates tests/fixtures/synth/*.txt.
// Usage: scripts/vanilla-java.sh crates/mc-worldgen/tests/oracle/SynthGolden.java crates/mc-worldgen/tests/fixtures/synth
// Output: crates/mc-worldgen/tests/fixtures/synth/{perlin,simplex,normal,blended,smeared}.txt (the program takes the output directory and writes the files into it directly; no -o wrapper)
// Every number is the raw hex bit pattern (double: 64-bit, float: 32-bit, ints: decimal).
import it.unimi.dsi.fastutil.doubles.DoubleArrayList;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Random;
import net.minecraft.util.Interval;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.densityfunction.DensityBuffer;
import net.minecraft.world.level.levelgen.densityfunction.DensitySampler;
import net.minecraft.world.level.levelgen.densityfunction.DensityVolume;
import net.minecraft.world.level.levelgen.densityfunction.SamplerContext;
import net.minecraft.world.level.levelgen.synth.BlendedNoise;
import net.minecraft.world.level.levelgen.synth.Noise;
import net.minecraft.world.level.levelgen.synth.NoiseStack;
import net.minecraft.world.level.levelgen.synth.NormalNoise;
import net.minecraft.world.level.levelgen.synth.PerlinNoise;
import net.minecraft.world.level.levelgen.synth.SimplexNoise;
import net.minecraft.world.level.levelgen.synth.SmearedPerlinNoise;

public class SynthGolden {
    static PrintWriter w;

    static String d(double v) { return Long.toHexString(Double.doubleToRawLongBits(v)); }
    static String f(float v) { return Integer.toHexString(Float.floatToRawIntBits(v)); }

    static final double[] SPECIAL = {
        0, 0.5, -0.5, 1, -1, 1.25, 255.99, -255.99, 3.7, -7.3, 100.7, 12345.678, -98765.4321,
        3e7, -3e7, 3.3554432E7, -3.3554432E7, 1.6777215999999998E7, 1.6777216E7, -1.6777216E7, 5e7, -1e9
    };

    static final double[][] VOLUME_SCALES = {{0.01, 0.02, 1.0}, {1.3, 0.7, 0.5}, {684.412 * 0.25, 684.412 * 0.125, 1.0}, {0.0078125, 0.0625, 0.75}};
    static final int[][] VOLUMES = {
        {5, 6, 4, -7, 60, 13, 1, 1, 1},
        {3, 9, 3, 0, -64, 0, 4, 8, 4},
        {2, 2, 2, 29999990, 0, -30000000, 16, 3, 5},
        {4, 17, 4, -123, -200, 987, 2, 1, 7},
    };

    static Random rng = new Random(20260416L);

    static double randomCoord() {
        double[] scales = {1, 16, 300, 1e4, 3e7};
        return (rng.nextDouble() * 2 - 1) * scales[rng.nextInt(scales.length)];
    }

    static final java.util.ArrayList<String> META = new java.util.ArrayList<>();

    /** Queues a metadata line emitted right after the next `case` header. */
    static void meta(String line) { META.add(line); }

    static void header(String name) {
        w.println("case " + name);
        META.forEach(w::println);
        META.clear();
    }

    static void range(Interval r) { w.println("range " + f(r.min()) + " " + f(r.max())); }

    static void noise(String name, Noise n) {
        header(name);
        range(n.range());
        for (int i = 0; i < 40; i++) {
            double x = SPECIAL[i % SPECIAL.length], y = SPECIAL[(i * 3 + 1) % SPECIAL.length];
            w.println("get2 " + d(x) + " " + d(y) + " " + f(n.get(x, y)));
        }
        for (int i = 0; i < 300; i++) {
            double x, y, z;
            if (i < 80) {
                x = SPECIAL[i % SPECIAL.length];
                y = SPECIAL[(i * 3 + 1) % SPECIAL.length];
                z = SPECIAL[(i * 7 + 2) % SPECIAL.length];
            } else {
                x = randomCoord(); y = randomCoord(); z = randomCoord();
            }
            if (i >= 80) {
                w.println("get2 " + d(x) + " " + d(z) + " " + f(n.get(x, z)));
            }
            w.println("get3 " + d(x) + " " + d(y) + " " + d(z) + " " + f(n.get(x, y, z)));
        }
        for (int v = 0; v < VOLUMES.length; v++) {
            int[] a = VOLUMES[v];
            double[] s = VOLUME_SCALES[v];
            DensityVolume vol = new DensityVolume(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8]);
            DensityBuffer buf = DensityBuffer.createUnpooled(vol.size());
            buf.fill(0.5F);
            n.addToVolume(buf, vol, s[0], s[1], (float) s[2]);
            w.println("volume " + String.join(" ", java.util.Arrays.stream(a).mapToObj(Integer::toString).toList())
                + " " + d(s[0]) + " " + d(s[1]) + " " + f((float) s[2]));
            StringBuilder sb = new StringBuilder("values");
            for (int i = 0; i < vol.size(); i++) sb.append(' ').append(f(buf.get(i)));
            w.println(sb);
        }
    }

    static void sampler(String name, DensitySampler sampler) {
        header(name);
        SamplerContext ctx = SamplerContext.EMPTY_UNCACHED;
        for (int i = 0; i < 120; i++) {
            int x = rng.nextInt(2001) - 1000, y = rng.nextInt(400) - 100, z = rng.nextInt(2001) - 1000;
            if (i < 8) { x = i % 2 == 0 ? 29999999 : -30000000; z = i < 4 ? 29999990 : -29999990; }
            w.println("value " + x + " " + y + " " + z + " " + f(sampler.sampleValue(ctx, x, y, z)));
        }
        for (int[] a : VOLUMES) {
            DensityVolume vol = new DensityVolume(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8]);
            DensityBuffer buf = DensityBuffer.createUnpooled(vol.size());
            sampler.sampleVolume(ctx, buf, vol);
            w.println("volume " + String.join(" ", java.util.Arrays.stream(a).mapToObj(Integer::toString).toList()));
            StringBuilder sb = new StringBuilder("values");
            for (int i = 0; i < vol.size(); i++) sb.append(' ').append(f(buf.get(i)));
            w.println(sb);
        }
    }

    static void params(NormalNoise.Normalization norm, double baseAmp, int baseOctave, int octaveCount, double[] mods) {
        StringBuilder sb = new StringBuilder("params " + d(baseAmp) + " " + baseOctave + " " + octaveCount + " " + norm.name().toLowerCase());
        for (double m : mods) sb.append(' ').append(d(m));
        meta(sb.toString());
    }

    static NormalNoise buildNormal(NormalNoise.Normalization norm, double baseAmp, int baseOctave, int octaveCount, double[] mods) {
        NormalNoise.Builder b = NormalNoise.builder().setBaseAmplitude(baseAmp).setBaseOctave(baseOctave).setOctaveCount(octaveCount);
        if (norm == NormalNoise.Normalization.DISABLED) b.setNormalize(false);
        if (norm == NormalNoise.Normalization.LEGACY) b.setLegacyNormalization();
        for (int i = 0; i < mods.length; i++) b.setAmplitudeModifier(i, mods[i]);
        return b.build();
    }

    static void normal(String name, NormalNoise.Normalization norm, double baseAmp, int baseOctave, int octaveCount, double[] mods, boolean legacyNether, long seed) {
        NormalNoise nn = buildNormal(norm, baseAmp, baseOctave, octaveCount, mods);
        params(norm, baseAmp, baseOctave, octaveCount, mods);
        meta("source " + (legacyNether ? "legacy " : "xoroshiro ") + seed);
        RandomSource r = legacyNether ? new LegacyRandomSource(seed) : new XoroshiroRandomSource(seed);
        Noise n = legacyNether ? nn.createForLegacyNetherBiome(r) : nn.create(r);
        meta("normal_range " + f(nn.range().min()) + " " + f(nn.range().max()));
        noise(name, n);
    }

    static void open(Path dir, String file) throws Exception { if (w != null) w.close(); w = new PrintWriter(Files.newBufferedWriter(dir.resolve(file))); }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        Path dir = Path.of(args[0]);

        open(dir, "perlin.txt");
        for (long seed : new long[] {0L, 1L, -987654321012345L, 20260416L}) {
            meta("source xoroshiro " + seed);
            noise("perlin_xoroshiro_" + seed, new PerlinNoise(new XoroshiroRandomSource(seed)));
            meta("source legacy " + seed);
            noise("perlin_legacy_" + seed, new PerlinNoise(new LegacyRandomSource(seed)));
        }

        open(dir, "simplex.txt");
        for (long seed : new long[] {0L, 12345L}) {
            meta("source legacy " + seed + " default");
            noise("simplex_default_" + seed, new SimplexNoise(new LegacyRandomSource(seed)));
            meta("source xoroshiro " + seed + " discard");
            noise("simplex_discard_" + seed, new SimplexNoise(new XoroshiroRandomSource(seed), true));
            meta("source xoroshiro " + seed + " keep");
            noise("simplex_keep_" + seed, new SimplexNoise(new XoroshiroRandomSource(seed), false));
        }

        open(dir, "smeared.txt");
        for (double fudge : new double[] {1.0, 8.55515, 2.0}) {
            meta("source xoroshiro 77");
            meta("fudge " + d(fudge));
            noise("smeared_" + fudge, new SmearedPerlinNoise(new XoroshiroRandomSource(77L), fudge));
            w.println("range_for " + d(fudge) + " " + f(SmearedPerlinNoise.range(fudge).max()));
        }

        open(dir, "normal.txt");
        long[] seeds = {0L, 987654321L};
        for (long seed : seeds) {
            normal("continentalness_" + seed, NormalNoise.Normalization.ENABLED, 0.8880832896205223, -9, 9, new double[] {1, 1, 2, 2, 2, 1, 1, 1, 1}, false, seed);
            normal("offset_" + seed, NormalNoise.Normalization.ENABLED, 0.9381732587751005, -3, 4, new double[] {1, 1, 1, 0}, false, seed);
            normal("temperature_" + seed, NormalNoise.Normalization.ENABLED, 1.2453007926713473, -10, 6, new double[] {1.5, 0, 1, 0, 0, 0}, false, seed);
            normal("jagged_" + seed, NormalNoise.Normalization.ENABLED, 1.0383104856073737, -16, 16, new double[0], false, seed);
            normal("disabled_" + seed, NormalNoise.Normalization.DISABLED, 1.0, -4, 3, new double[] {1, 0.5, 0.25}, false, seed);
            normal("legacy_" + seed, NormalNoise.Normalization.LEGACY, 1.0, -7, 2, new double[0], false, seed);
            normal("legacy_mods_" + seed, NormalNoise.Normalization.LEGACY, 0.75, -5, 4, new double[] {1, 0, 2, 1}, false, seed);
            normal("nether_temperature_" + seed, NormalNoise.Normalization.ENABLED, 0.9494731054427981, -7, 2, new double[0], true, seed);
            normal("nether_mods_" + seed, NormalNoise.Normalization.ENABLED, 1.3, -6, 5, new double[] {1, 0, 1.5, 1, 0}, true, seed);
        }
        // createParity
        {
            NormalNoise p = NormalNoise.createParity(-7, 1.0, 1.0);
            meta("parity -7 " + d(1.0) + " " + d(1.0));
            meta("normal_range " + f(p.range().min()) + " " + f(p.range().max()));
            meta("source xoroshiro 5");
            noise("parity_-7_1_1", p.create(new XoroshiroRandomSource(5L)));
            NormalNoise p2 = NormalNoise.createParity(-6, 1.0, 0.0, 2.0, 0.5);
            meta("parity -6 " + d(1.0) + " " + d(0.0) + " " + d(2.0) + " " + d(0.5));
            meta("normal_range " + f(p2.range().min()) + " " + f(p2.range().max()));
            meta("source xoroshiro 5");
            noise("parity_-6_mods", p2.create(new XoroshiroRandomSource(5L)));
        }

        open(dir, "blended.txt");
        double[][] blended = {{0.25, 0.125, 80, 160, 8}, {0.9, 2.0, 40, 30, 3.0}, {1, 1, 1, 1, 1}};
        for (double[] b : blended) {
            BlendedNoise bn = new BlendedNoise(b[0], b[1], b[2], b[3], b[4]);
            String params = d(b[0]) + " " + d(b[1]) + " " + d(b[2]) + " " + d(b[3]) + " " + d(b[4]);
            long seed = 4242L;
            BlendedNoise.FbmSet set = bn.createFbmSet(new XoroshiroRandomSource(seed));
            for (String kind : new String[] {"min", "max", "main"}) {
                meta("blended " + params + " " + seed);
                meta("df_range " + f(bn.range().min()) + " " + f(bn.range().max()));
                meta("fbm " + kind);
                noise("blended_" + kind + "_" + b[0], kind.equals("min") ? set.minLimitNoise() : kind.equals("max") ? set.maxLimitNoise() : set.mainNoise());
            }
            meta("blended " + params + " " + seed);
            sampler("blended_sampler_" + b[0], bn.compileSampler(new XoroshiroRandomSource(seed)));
        }
        w.close();
    }
}
