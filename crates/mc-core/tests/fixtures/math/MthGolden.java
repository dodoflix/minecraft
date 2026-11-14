// Generates mth.txt.
// Usage: scripts/vanilla-java.sh -o crates/mc-core/tests/fixtures/math/mth.txt.gz crates/mc-core/tests/fixtures/math/MthGolden.java
// Output: crates/mc-core/tests/fixtures/math/mth.txt.gz
// Line format: `<fn> <args...> = <result>`; double = 16 hex raw bits, float = 8 hex raw bits, int/long decimal, bool true/false, int lists `[a,b]`.
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.util.Mth;
import net.minecraft.util.RandomSource;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

public class MthGolden {
   static StringBuilder out = new StringBuilder();

   static String t(Object o) {
      if (o instanceof Double d) return String.format(java.util.Locale.ROOT, "%016x", Double.doubleToRawLongBits(d));
      if (o instanceof Float f) return String.format(java.util.Locale.ROOT, "%08x", Float.floatToRawIntBits(f));
      if (o instanceof int[] a) return Arrays.toString(a).replace(" ", "");
      return String.valueOf(o);
   }

   static void e(String name, Object r, Object... args) {
      out.append(name);
      for (Object a : args) out.append(' ').append(t(a));
      out.append(" = ").append(t(r)).append('\n');
   }

   static final double[] SPECIAL_D = {
      0.0, -0.0, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.MAX_VALUE, -Double.MAX_VALUE, Double.MIN_VALUE,
      0.5, -0.5, 1.0, -1.0, 2.0, 3.0, 45.0, 90.0, 179.99, 180.0, -180.0, 360.0, -360.0, 540.0, 1e-5, 1e18, -1e18, 9.3e18, Math.PI, -Math.PI
   };
   static final float[] SPECIAL_F = {
      0.0f, -0.0f, Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY, Float.MAX_VALUE, Float.MIN_VALUE,
      0.5f, -0.5f, 1.0f, -1.0f, 2.0f, 45.0f, 90.0f, 180.0f, -180.0f, 360.0f, -360.0f, 540.0f, 1e-5f, (float) Math.PI, 1e10f, -1e10f
   };

   static double rd(Random r) {
      switch (r.nextInt(7)) {
         case 0: return r.nextGaussian() * 10;
         case 1: return r.nextGaussian() * 1000;
         case 2: return (r.nextDouble() - 0.5) * 1e10;
         case 3: return (r.nextDouble() - 0.5) * 7;
         case 4: return (r.nextDouble() - 0.5) * 4;
         case 5: return r.nextGaussian() * 1e5;
         default: return Double.longBitsToDouble(r.nextLong());
      }
   }

   static float rf(Random r) {
      switch (r.nextInt(6)) {
         case 0: return (float) (r.nextGaussian() * 10);
         case 1: return (float) (r.nextGaussian() * 1000);
         case 2: return (r.nextFloat() - 0.5f) * 1e6f;
         case 3: return (r.nextFloat() - 0.5f) * 7;
         case 4: return (r.nextFloat() - 0.5f) * 4;
         default: return Float.intBitsToFloat(r.nextInt());
      }
   }

   static double d(Random r, int i) { return i < SPECIAL_D.length ? SPECIAL_D[i] : rd(r); }
   static float f(Random r, int i) { return i < SPECIAL_F.length ? SPECIAL_F[i] : rf(r); }
   static float unit(Random r) { return r.nextFloat(); }

   static int ri(Random r) {
      switch (r.nextInt(5)) {
         case 0: return r.nextInt(100) - 50;
         case 1: return r.nextInt(100000) - 50000;
         case 2: return r.nextInt();
         case 3: return r.nextInt(5000);
         default: return switch (r.nextInt(4)) { case 0 -> Integer.MIN_VALUE; case 1 -> Integer.MAX_VALUE; case 2 -> 0; default -> -1; };
      }
   }

   static long rl(Random r) { return r.nextInt(3) == 0 ? r.nextLong() : (long) ri(r) * (r.nextInt(2) == 0 ? 1 : 100000L); }

   public static void main(String[] args) throws Exception {
      Random r = new Random(0x5EEDL);
      // sin/cos table hashes (full 65536 entries via reflection)
      Field sf = Mth.class.getDeclaredField("SIN");
      sf.setAccessible(true);
      float[] sin = (float[]) sf.get(null);
      long hs = 0, hc = 0;
      for (int i = 0; i < 65536; i++) {
         hs = hs * 1000003L + Float.floatToRawIntBits(sin[i]);
         hc = hc * 1000003L + Float.floatToRawIntBits(sin[(i + 16384) & 65535]);
      }
      out.append("sintable_hash ").append(hs).append(' ').append(hc).append('\n');
      for (String name : new String[]{"ASIN_TAB", "COS_TAB"}) {
         Field tf = Mth.class.getDeclaredField(name);
         tf.setAccessible(true);
         double[] tab = (double[]) tf.get(null);
         long h = 0;
         for (double v : tab) h = h * 1000003L + Double.doubleToRawLongBits(v);
         out.append(name.toLowerCase(java.util.Locale.ROOT)).append("_hash ").append(h).append('\n');
      }
      e("consts", "x", Mth.PI, Mth.HALF_PI, Mth.TWO_PI, Mth.DEG_TO_RAD, Mth.RAD_TO_DEG, Mth.EPSILON, Mth.SQRT_OF_TWO);

      for (int i = 0; i < 2000; i++) {
         double x = d(r, i);
         e("sin", Mth.sin(x), x);
         e("cos", Mth.cos(x), x);
      }
      for (int i = 0; i < 2000; i++) {
         double y = d(r, i % 40), x = i < 800 ? d(r, i / 28) : d(r, 100);
         if (i % 3 == 0) { y = r.nextGaussian(); x = r.nextGaussian(); }
         if (i % 7 == 0) { y = (r.nextInt(200) - 100) / 10.0; x = (r.nextInt(200) - 100) / 10.0; }
         e("atan2", Mth.atan2(y, x), y, x);
      }
      for (int k = 0; k < 3000; k++) {
         double ang = k * Math.PI / 1500.0, rad = k % 3 == 0 ? 1.0 : 0.001 * (1 + k % 997);
         e("atan2", Mth.atan2(rad * Math.sin(ang), rad * Math.cos(ang)), rad * Math.sin(ang), rad * Math.cos(ang));
      }
      for (int i = 0; i < 600; i++) {
         double x = i < SPECIAL_D.length ? SPECIAL_D[i] : Math.abs(rd(r));
         e("fastInvSqrt", Mth.fastInvSqrt(x), x);
         e("invSqrt_d", Mth.invSqrt(x), x);
         e("lfloor", Mth.lfloor(x), x);
         e("floor_d", Mth.floor(x), x);
         e("ceil_d", Mth.ceil(x), x);
         e("ceilLong", Mth.ceilLong(x), x);
         e("frac_d", Mth.frac(x), x);
         e("sign", Mth.sign(x), x);
         e("wrapDegrees_d", Mth.wrapDegrees(x), x);
         e("quantize", Mth.quantize(x, 16), x, 16);
         e("quantize", Mth.quantize(x, 3), x, 3);
      }
      for (int i = 0; i < 800; i++) {
         float x = f(r, i);
         e("fastInvCubeRoot", Mth.fastInvCubeRoot(x), x);
         e("invSqrt_f", Mth.invSqrt(x), x);
         e("sqrt", Mth.sqrt(x), x);
         e("floor_f", Mth.floor(x), x);
         e("ceil_f", Mth.ceil(x), x);
         e("frac_f", Mth.frac(x), x);
         e("wrapDegrees_f", Mth.wrapDegrees(x), x);
         e("wrapDegrees90", Mth.wrapDegrees90(x), x);
         e("smoothstep", Mth.smoothstep(x), x);
         e("smoothstepDerivative", Mth.smoothstepDerivative(x), x);
         e("packDegrees", (int) Mth.packDegrees(x), x);
         e("abs_f", Mth.abs(x), x);
         e("square_f", Mth.square(x), x);
         e("cube", Mth.cube(x), x);
      }
      for (int i = 0; i < 600; i++) {
         int x = ri(r);
         e("murmur", Mth.murmurHash3Mixer(x), x);
         e("ceillog2", Mth.ceillog2(x), x);
         e("log2", Mth.log2(x), x);
         e("smallestEncompassingPowerOfTwo", Mth.smallestEncompassingPowerOfTwo(x), x);
         e("isPowerOfTwo_i", Mth.isPowerOfTwo(x), x);
         e("wrapDegrees_i", Mth.wrapDegrees(x), x);
         e("abs_i", Mth.abs(x), x);
         e("square_i", Mth.square(x), x);
         e("unpackDegrees", Mth.unpackDegrees((byte) x), (int) (byte) x);
         long l = rl(r);
         e("wrapDegrees_l", Mth.wrapDegrees(l), l);
         e("isPowerOfTwo_l", Mth.isPowerOfTwo(l), l);
         e("square_l", Mth.square(l), l);
         int m = ri(r);
         if (m == 0) m = 7;
         e("positiveModulo_i", Mth.positiveModulo(x, m), x, m);
         e("floorDiv", Mth.floorDiv(x, m), x, m);
         e("isMultipleOf", Mth.isMultipleOf(x, m), x, m);
         e("positiveCeilDiv_i", Mth.positiveCeilDiv(x, m), x, m);
         e("roundToward_i", Mth.roundToward(x, m), x, m);
         long lm = rl(r);
         if (lm == 0) lm = 5;
         e("positiveCeilDiv_l", Mth.positiveCeilDiv(l, lm), l, lm);
         e("roundToward_l", Mth.roundToward(l, lm), l, lm);
         e("absMax_i", Mth.absMax(x, m), x, m);
         e("clamp_i", Mth.clamp(x, Math.min(m, x), Math.max(m, x)), x, Math.min(m, x), Math.max(m, x));
         int a = ri(r), b = ri(r);
         e("clamp_i", Mth.clamp(x, a, b), x, a, b);
         long lmin = rl(r);
         e("clamp_l", Mth.clamp(l, lmin, lm), l, lmin, lm);
         e("chessboardDistance", Mth.chessboardDistance(x, m, a, b), x, m, a, b);
         e("getSeed", Mth.getSeed(x, m, a), x, m, a);
         int sm = Math.abs(x % 100000);
         e("smallestSquareSide", Mth.smallestSquareSide(sm), sm);
      }
      for (int i = 0; i < 600; i++) {
         double a = d(r, i), b = d(r, i / 2 + 3), c = d(r, 50 + i);
         float fa = f(r, i), fb = f(r, i / 2 + 3), fc = f(r, 50 + i);
         e("clamp_d", Mth.clamp(a, b, c), a, b, c);
         e("clamp_f", Mth.clamp(fa, fb, fc), fa, fb, fc);
         e("clampedLerp_d", Mth.clampedLerp(a, b, c), a, b, c);
         e("clampedLerp_f", Mth.clampedLerp(fa, fb, fc), fa, fb, fc);
         e("lerp_d", Mth.lerp(a, b, c), a, b, c);
         e("lerp_f", Mth.lerp(fa, fb, fc), fa, fb, fc);
         e("inverseLerp_d", Mth.inverseLerp(a, b, c), a, b, c);
         e("inverseLerp_f", Mth.inverseLerp(fa, fb, fc), fa, fb, fc);
         e("absMax_d", Mth.absMax(a, b), a, b);
         e("absMax_f", Mth.absMax(fa, fb), fa, fb);
         e("equal_d", Mth.equal(a, b), a, b);
         e("equal_f", Mth.equal(fa, fb), fa, fb);
         e("positiveModulo_d", Mth.positiveModulo(a, b), a, b);
         e("positiveModulo_f", Mth.positiveModulo(fa, fb), fa, fb);
         e("rotLerp_d", Mth.rotLerp(a, b, c), a, b, c);
         e("rotLerp_f", Mth.rotLerp(fa, fb, fc), fa, fb, fc);
         e("rotLerpRad", Mth.rotLerpRad(fa % 4, fb % 20, fc % 20), fa % 4, fb % 20, fc % 20);
         e("triangleWave", Mth.triangleWave(fa, fb), fa, fb);
         e("approach", Mth.approach(fa, fb, fc), fa, fb, fc);
         e("approachDegrees", Mth.approachDegrees(fa, fb, fc), fa, fb, fc);
         e("degreesDifference", Mth.degreesDifference(fa, fb), fa, fb);
         e("degreesDifferenceAbs", Mth.degreesDifferenceAbs(fa, fb), fa, fb);
         e("rotateIfNecessary", Mth.rotateIfNecessary(fa, fb, fc), fa, fb, fc);
         e("map_d", Mth.map(a, b, c, a, c), a, b, c, a, c);
         e("map_f", Mth.map(fa, fb, fc, fb, fa), fa, fb, fc, fb, fa);
         e("clampedMap_d", Mth.clampedMap(a, b, c, c, a), a, b, c, c, a);
         e("clampedMap_f", Mth.clampedMap(fa, fb, fc, fc, fa), fa, fb, fc, fc, fa);
         e("catmullrom", Mth.catmullrom(fa, fb, fc, fa, fb), fa, fb, fc, fa, fb);
         e("lerp2_d", Mth.lerp2(a, b, c, a, b, c), a, b, c, a, b, c);
         e("lerp2_f", Mth.lerp2(fa, fb, fc, fa, fb, fc), fa, fb, fc, fa, fb, fc);
         e("lerp3_d", Mth.lerp3(a, b, c, a, b, c, a, b, c, a, b), a, b, c, a, b, c, a, b, c, a, b);
         e("lerp3_f", Mth.lerp3(fa, fb, fc, fa, fb, fc, fa, fb, fc, fa, fb), fa, fb, fc, fa, fb, fc, fa, fb, fc, fa, fb);
         e("lengthSquared2_d", Mth.lengthSquared(a, b), a, b);
         e("lengthSquared2_f", Mth.lengthSquared(fa, fb), fa, fb);
         e("length2_d", Mth.length(a, b), a, b);
         e("length2_f", Mth.length(fa, fb), fa, fb);
         e("lengthSquared3_d", Mth.lengthSquared(a, b, c), a, b, c);
         e("lengthSquared3_f", Mth.lengthSquared(fa, fb, fc), fa, fb, fc);
         e("length3_d", Mth.length(a, b, c), a, b, c);
         e("length3_f", Mth.length(fa, fb, fc), fa, fb, fc);
      }
      for (int i = 0; i < 600; i++) {
         float h = r.nextFloat(), s = i % 5 == 0 ? 1.0f : r.nextFloat(), v = i % 7 == 0 ? 1.0f : r.nextFloat();
         if (i % 11 == 0) h = (i % 6) / 6.0f;
         e("hsvToRgb", Mth.hsvToRgb(h, s, v), h, s, v);
         int al = r.nextInt(300) - 20;
         e("hsvToArgb", Mth.hsvToArgb(h, s, v, al), h, s, v, al);
         float al1 = r.nextFloat(), p0 = ri(r) / 4, p1 = ri(r) / 4;
         int ip0 = r.nextInt(2000) - 1000, ip1 = r.nextInt(2000) - 1000;
         e("lerpInt", Mth.lerpInt(al1, ip0, ip1), al1, ip0, ip1);
         e("lerpDiscrete", Mth.lerpDiscrete(al1, ip0, ip1), al1, ip0, ip1);
         e("lerpDiscrete", Mth.lerpDiscrete(0.0f, ip0, ip1), 0.0f, ip0, ip1);
      }
      for (int i = 0; i < 800; i++) {
         double[] v = new double[12];
         for (int k = 0; k < 12; k++) v[k] = (r.nextInt(5) == 0 ? Math.round(r.nextGaussian() * 4) : r.nextGaussian() * 4);
         AABB box = new AABB(v[6], v[7], v[8], v[9], v[10], v[11]);
         boolean hit = Mth.rayIntersectsAABB(new Vec3(v[0], v[1], v[2]), new Vec3(v[3], v[4], v[5]), box);
         e("ray", hit, v[0], v[1], v[2], v[3], v[4], v[5], box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ);
         Vec3 lv = Mth.lerp(v[0] / 4, new Vec3(v[1], v[2], v[3]), new Vec3(v[4], v[5], v[6]));
         e("lerp_vec3", t(lv.x) + " " + t(lv.y) + " " + t(lv.z), v[0] / 4, v[1], v[2], v[3], v[4], v[5], v[6]);
      }
      // RandomSource-taking helpers on a LegacyRandomSource: fixed call sequence per seed
      for (int i = 0; i < 200; i++) {
         long seed = r.nextLong();
         RandomSource rs = RandomSource.create(seed);
         int lo = r.nextInt(100) - 50, hi = lo + r.nextInt(5) - 1 + (i % 3 == 0 ? 40 : 0);
         float flo = r.nextFloat() * 10 - 5, fhi = flo + (i % 5 == 0 ? -1 : r.nextFloat() * 10);
         double dlo = r.nextDouble() * 10 - 5, dhi = dlo + (i % 4 == 0 ? -1 : r.nextDouble() * 10);
         String res = Mth.nextInt(rs, lo, hi) + " " + t(Mth.nextFloat(rs, flo, fhi)) + " " + t(Mth.nextDouble(rs, dlo, dhi)) + " "
            + Mth.randomBetweenInclusive(rs, lo, hi + 1) + " " + t(Mth.randomBetween(rs, flo, fhi + 1)) + " " + t(Mth.normal(rs, flo, fhi)) + " "
            + Mth.createInsecureUUID(rs);
         e("rnd", res, seed, lo, hi, flo, fhi, dlo, dhi);
      }
      for (int i = 0; i < 300; i++) {
         double x = i < 40 ? i * 0.37 - 7 : rd(r) / 100;
         e("wobble", Mth.wobble(x), x);
      }
      // binarySearch: first index in [from,to) with x*x >= target
      for (int i = 0; i < 300; i++) {
         int from = r.nextInt(200) - 100, to = from + r.nextInt(300);
         int target = r.nextInt(20000);
         e("binarySearch", Mth.binarySearch(from, to, n -> (long) n * n >= target), from, to, target);
      }
      // outFromOrigin
      for (int i = 0; i < 300; i++) {
         int lo = r.nextInt(40) - 20, hi = lo + r.nextInt(30), origin = r.nextInt(80) - 40, step = 1 + r.nextInt(4);
         e("outFromOrigin", Mth.outFromOrigin(origin, lo, hi).toArray(), origin, lo, hi);
         e("outFromOrigin_step", Mth.outFromOrigin(origin, lo, hi, step).toArray(), origin, lo, hi, step);
      }
      // Math.atan2/asin/acos/atan (StrictMath fdlibm); own generator so the other sections keep their inputs
      Random m = new Random(0xA7A2L);
      for (double y : SPECIAL_D) for (double x : SPECIAL_D) e("m_atan2", Math.atan2(y, x), y, x);
      for (int i = 0; i < 6000; i++) {
         double y = rd(m), x = rd(m);
         if (i % 3 == 0) { y = m.nextGaussian(); x = m.nextGaussian(); }
         if (i % 5 == 0) { y = (m.nextInt(200) - 100) / 10.0; x = (m.nextInt(200) - 100) / 10.0; }
         e("m_atan2", Math.atan2(y, x), y, x);
      }
      for (int i = 0; i < 8000; i++) {
         double v = i < SPECIAL_D.length ? SPECIAL_D[i] : switch (i % 4) {
            case 0 -> m.nextDouble() * 2 - 1;
            case 1 -> (m.nextDouble() * 2 - 1) * (m.nextBoolean() ? 1e-3 : 1.0000001);
            case 2 -> (m.nextInt(2001) - 1000) / 1000.0;
            default -> rd(m);
         };
         e("m_asin", Math.asin(v), v);
         e("m_acos", Math.acos(v), v);
         e("m_atan", Math.atan(v), v);
      }
      // hsv error case: hue 1.0 -> sector 6
      try { Mth.hsvToRgb(-0.5f, 0.5f, 0.5f); out.append("hsv_err bf000000 = ok\n"); }
      catch (RuntimeException ex) { out.append("hsv_err bf000000 = ").append(ex.getMessage()).append('\n'); }
      System.out.print(out);
   }
}
