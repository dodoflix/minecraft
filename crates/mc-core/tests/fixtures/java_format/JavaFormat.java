import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/** Prints `d <bits16> <Double.toString>`, `f <bits8> <Float.toString>`, `v3 <3 bits> <Vec3.toString>`, `aabb <6 bits> <AABB.toString>`. */
public class JavaFormat {
    static StringBuilder out = new StringBuilder();
    static String hx(double d) { return String.format("%016x", Double.doubleToRawLongBits(d)); }
    static String hf(float f) { return String.format("%08x", Float.floatToRawIntBits(f)); }
    static void d(double v) { out.append("d ").append(hx(v)).append(' ').append(Double.toString(v)).append('\n'); }
    static void f(float v) { out.append("f ").append(hf(v)).append(' ').append(Float.toString(v)).append('\n'); }

    public static void main(String[] a) throws Exception {
        double[] ed = {0.0, -0.0, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.MIN_VALUE, Double.MAX_VALUE,
            Double.MIN_NORMAL, 0.1 + 0.2, 1e23, 9007199254740993.0, 9.223372036854775807E18, 2e-323, 1e-323, 3e-323, 5e-324 * 3,
            Math.PI, Math.E, 100.0, 1.0, 0.5, 123456.789, 1.0E7, 9999999.999999998, 0.001, 0.00099999999999999, 1e-5, 1.234e-5, 1e22, 1e21, 2e23, 8.41E21, 5e-324, 4.9e-324};
        for (double v : ed) { d(v); d(-v); }
        for (int e = -1074; e <= 1023; e++) { double p = Math.scalb(1.0, e); d(p); d(Math.nextUp(p)); d(Math.nextDown(p)); }
        for (int e = -330; e <= 310; e++) { double p = Double.parseDouble("1e" + e); d(p); d(Math.nextUp(p)); d(Math.nextDown(p)); d(p * 2); d(p * 5); d(p * 1.5); }
        for (double p : new double[]{1e-3, 1e7, 1e-2, 1e6, 9999999, 10000000, 0.0009999999999999998, 1e-4}) { d(p); d(Math.nextUp(p)); d(Math.nextDown(p)); }
        // subnormal ladder
        for (long i = 1; i <= 2000; i++) d(Double.longBitsToDouble(i));
        for (long i = 1; i <= 2000; i++) d(Double.longBitsToDouble(i * 1000003L));
        float[] ef = {0f, -0f, Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY, Float.MIN_VALUE, Float.MAX_VALUE, Float.MIN_NORMAL,
            0.1f + 0.2f, 1e10f, 1e23f, 16777217f, 1.0E7f, 9999999f, 0.001f, 0.00099999f, 1e-5f, 1.234e-5f, 3.4028235e38f, 2e-45f, 1e-45f, 3e-45f,
            (float) Math.PI, 100f, 1f, 0.5f, 123456.79f, 8.5e-44f, 1.1754944e-38f, 2.0e-44f};
        for (float v : ef) { f(v); f(-v); }
        for (int e = -149; e <= 127; e++) { float p = Math.scalb(1.0f, e); f(p); f(Math.nextUp(p)); f(Math.nextDown(p)); }
        for (int e = -46; e <= 39; e++) { float p = Float.parseFloat("1e" + e); f(p); f(Math.nextUp(p)); f(Math.nextDown(p)); f(p * 2); f(p * 5); f(p * 1.5f); }
        for (int i = 1; i <= 2000; i++) f(Float.intBitsToFloat(i));
        Random r = new Random(0x5EEDF00DL);
        for (int i = 0; i < 20000; i++) d(Double.longBitsToDouble(r.nextLong()));
        for (int i = 0; i < 20000; i++) f(Float.intBitsToFloat(r.nextInt()));
        // "human" values: short decimals, ints, fractions
        for (int i = 0; i < 3000; i++) { d(r.nextInt(100000) / 1000.0); d(r.nextInt(1000) * 1e5 / 7); f(r.nextInt(100000) / 1000f); f(r.nextInt(1000) * 1e5f / 7); }

        double[][] v3 = {{0, 0, 0}, {1.5, -2.25, 1e7}, {0.1 + 0.2, 1e-4, -0.0}, {Double.NaN, Double.POSITIVE_INFINITY, Double.MIN_VALUE}, {123456.789, 9.99e-4, 1e21}};
        for (double[] t : v3) {
            Vec3 v = new Vec3(t[0], t[1], t[2]);
            out.append("v3 ").append(hx(t[0])).append(' ').append(hx(t[1])).append(' ').append(hx(t[2])).append(' ').append(v).append('\n');
        }
        double[][] bb = {{0, 0, 0, 1, 1, 1}, {-0.5, 64, 1e-5, 0.30000000000000004, 1.0E10, 2e-323}, {1.5, 2.5, 3.5, 4.5, 5.5, 6.5}, {-1e7, 9999999.5, -0.001, 0.0009, 5e-324, 1.7976931348623157E308}};
        for (double[] t : bb) {
            AABB b = new AABB(t[0], t[1], t[2], t[3], t[4], t[5]);
            out.append("aabb");
            for (double x : t) out.append(' ').append(hx(x));
            out.append(' ').append(b).append('\n');
        }
        try (Writer w = new OutputStreamWriter(new FileOutputStream(a[0]), StandardCharsets.UTF_8)) { w.write(out.toString()); }
    }
}
