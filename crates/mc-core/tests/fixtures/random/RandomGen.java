import net.minecraft.world.level.levelgen.*;
import net.minecraft.util.RandomSource;

// Usage: scripts/vanilla-java.sh RandomGen.java > random.txt
public class RandomGen {
    static final long[] SEEDS = {0L, 1L, -1L, 42L, 12345L, -987654321L, 0x123456789ABCDEFL, Long.MAX_VALUE, Long.MIN_VALUE};
    static final int[] BOUNDS = {1, 2, 3, 7, 16, 100, 1000, 65536, 1 << 30, Integer.MAX_VALUE, 12345678};
    static final StringBuilder SB = new StringBuilder();

    static void p(String tag, Object v) { SB.append(tag).append(' ').append(v).append('\n'); }
    static String f(float v) { return Integer.toHexString(Float.floatToRawIntBits(v)); }
    static String d(double v) { return Long.toHexString(Double.doubleToRawLongBits(v)); }

    static RandomSource make(String kind, long seed) {
        return switch (kind) {
            case "legacy" -> new LegacyRandomSource(seed);
            case "threadsafe" -> new ThreadSafeLegacyRandomSource(seed);
            case "single" -> new SingleThreadedRandomSource(seed);
            case "xoroshiro" -> new XoroshiroRandomSource(seed);
            case "wg_legacy" -> new WorldgenRandom(new LegacyRandomSource(seed));
            case "wg_xoroshiro" -> new WorldgenRandom(new XoroshiroRandomSource(seed));
            case "wg_single" -> new WorldgenRandom(new SingleThreadedRandomSource(seed));
            case "wg_threadsafe" -> new WorldgenRandom(new ThreadSafeLegacyRandomSource(seed));
            case "wg_wg" -> new WorldgenRandom(new WorldgenRandom(new LegacyRandomSource(seed)));
            default -> throw new IllegalArgumentException(kind);
        };
    }

    static void child(String tag, RandomSource r) {
        p(tag + ".l", r.nextLong() + " " + r.nextLong() + " " + r.nextLong());
        p(tag + ".i", r.nextInt() + " " + r.nextInt(100));
        p(tag + ".d", d(r.nextDouble()));
    }

    static void run(String kind, long seed) {
        String t = kind + " " + seed + " ";
        RandomSource r = make(kind, seed);
        p(t + "int", r.nextInt() + " " + r.nextInt() + " " + r.nextInt() + " " + r.nextInt());
        for (int b : BOUNDS) p(t + "bound" + b, r.nextInt(b) + " " + r.nextInt(b) + " " + r.nextInt(b));
        p(t + "between_incl", r.nextIntBetweenInclusive(-5, 5) + " " + r.nextIntBetweenInclusive(10, 10) + " " + r.nextIntBetweenInclusive(0, 1000));
        p(t + "between", r.nextInt(-100, 100) + " " + r.nextInt(0, 1) + " " + r.nextInt(5, 99999));
        p(t + "long", r.nextLong() + " " + r.nextLong() + " " + r.nextLong());
        p(t + "bool", r.nextBoolean() + " " + r.nextBoolean() + " " + r.nextBoolean() + " " + r.nextBoolean());
        p(t + "float", f(r.nextFloat()) + " " + f(r.nextFloat()) + " " + f(r.nextFloat()));
        p(t + "double", d(r.nextDouble()) + " " + d(r.nextDouble()) + " " + d(r.nextDouble()));
        for (int i = 0; i < 5; i++) p(t + "gauss" + i, d(r.nextGaussian()));
        r.setSeed(seed + 1);
        p(t + "gauss_after_reseed", d(r.nextGaussian()) + " " + d(r.nextDouble()));
        r.nextGaussian(); // leave a cached gaussian, then reseed again
        r.setSeed(seed);
        p(t + "gauss_after_reseed2", d(r.nextGaussian()));
        p(t + "triangle", d(r.triangle(1.5, 2.5)) + " " + f(r.triangle(1.0f, 3.0f)));
        r.consumeCount(7);
        p(t + "after_consume", r.nextInt());
        RandomSource fk = r.fork();
        child(t + "fork", fk);
        p(t + "parent_after_fork", r.nextLong() + " " + r.nextLong());
        PositionalRandomFactory pf = r.forkPositional();
        StringBuilder cfg = new StringBuilder();
        pf.parityConfigString(cfg);
        p(t + "pf_cfg", cfg);
        child(t + "at000", pf.at(0, 0, 0));
        child(t + "at123", pf.at(1, 2, 3));
        child(t + "atneg", pf.at(-5, 100, -30000000));
        child(t + "atmax", pf.at(Integer.MAX_VALUE, Integer.MIN_VALUE, 29999999));
        child(t + "hash_overworld", pf.fromHashOf("minecraft:overworld"));
        child(t + "hash_utf", pf.fromHashOf("h\u00e9llo \uD83D\uDE00"));
        child(t + "hash_empty", pf.fromHashOf(""));
        child(t + "fromseed", pf.fromSeed(42L));
        p(t + "end", r.nextInt());
        if (r instanceof WorldgenRandom w) p(t + "count", w.getCount());
    }

    public static void main(String[] a) {
        net.minecraft.SharedConstants.tryDetectVersion();
        for (String kind : new String[]{"legacy", "threadsafe", "single", "xoroshiro", "wg_legacy", "wg_xoroshiro", "wg_single", "wg_threadsafe", "wg_wg"})
            for (long s : SEEDS) run(kind, s);

        for (long s : SEEDS) {
            RandomSupport.Seed128bit u = RandomSupport.upgradeSeedTo128bit(s);
            RandomSupport.Seed128bit um = RandomSupport.upgradeSeedTo128bitUnmixed(s);
            p("upgrade " + s, u.seedLo() + " " + u.seedHi() + " " + um.seedLo() + " " + um.seedHi() + " " + RandomSupport.mixStafford13(s));
            XoroshiroRandomSource x0 = new XoroshiroRandomSource(0L, 0L);
            p("xoro_zero", x0.nextLong());
        }
        for (String s : new String[]{"", "a", "minecraft:overworld", "h\u00e9llo \uD83D\uDE00", "minecraft:ore_iron"}) {
            RandomSupport.Seed128bit h = RandomSupport.seedFromHashOf(s);
            p("hash " + s.length() + " " + s.hashCode(), h.seedLo() + " " + h.seedHi());
        }

        int[] coords = {0, 1, -3, 5, 1000, -30000000, 29999999, Integer.MAX_VALUE, Integer.MIN_VALUE};
        for (WorldgenRandom.Algorithm alg : WorldgenRandom.Algorithm.values()) {
            for (long s : SEEDS) {
                for (int x : coords) for (int z : coords) {
                    String t = "wg " + alg + " " + s + " " + x + " " + z + " ";
                    WorldgenRandom w = new WorldgenRandom(alg.newInstance(0L));
                    long dec = w.setDecorationSeed(s, x, z);
                    p(t + "decoration", dec + " " + w.nextLong() + " " + w.getCount());
                    w.setFeatureSeed(s, x, z);
                    p(t + "feature", w.nextLong());
                    w.setLargeFeatureSeed(s, x, z);
                    p(t + "large", w.nextLong());
                    w.setLargeFeatureWithSalt(s, x, z, 10387312);
                    p(t + "large_salt", w.nextLong());
                    RandomSource sl = WorldgenRandom.seedSlimeChunk(x, z, s, 987234911L);
                    p(t + "slime", sl.nextInt(10) + " " + sl.nextLong());
                }
            }
        }
        System.out.print(SB);
    }
}
