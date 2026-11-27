// Java-literal golden of the `java.lang.Math` functions the server sources call, for bit-exact comparison with the Rust ports.
// Usage: java crates/mc-core/tests/fixtures/math/MathGolden.java > crates/mc-core/tests/fixtures/math/math_intrinsics.txt   (scripts/golden-math.sh)
//        java crates/mc-core/tests/fixtures/math/MathGolden.java full CASE     every input and output of one case, hex bits, one pair per line
// Output: crates/mc-core/tests/fixtures/math/math_intrinsics.txt.gz
// Format: `case NAME COUNT BLOCK` then one 16-digit FNV-1a digest of the outputs per BLOCK inputs. Inputs come from SplitMix64 seeded
// with the FNV-1a of the case name, mixing raw bit patterns, ranges, powers of two, multiples of pi/2 and integers (see `mix`);
// the first inputs are the edge values. Outputs are hashed as `doubleToLongBits`/`floatToIntBits` (canonical NaN) or as the integer.
// The Rust side (golden/math_intrinsics.rs) generates the same inputs with the same code.
import java.util.ArrayList;
import java.util.List;

public class MathGolden {
   static final int COUNT = 1 << 21;
   static final int BLOCK = 1024;

   interface Fn {
      long apply(long a, long b);
   }

   record Case(String name, char gen, Fn fn) {}

   // gen: 'd' one double, 'D' two doubles, 'p' pow(x, y) with small y, 'i' pow(x, n) with an int n, 'f' one float, 'F' two floats, 'I' two ints, 'L' two longs
   static final List<Case> CASES = new ArrayList<>();

   static long dbl(double v) {
      return Double.doubleToLongBits(v);
   }

   static double d(long bits) {
      return Double.longBitsToDouble(bits);
   }

   static long flt(float v) {
      return Float.floatToIntBits(v);
   }

   static float f(long bits) {
      return Float.intBitsToFloat((int)bits);
   }

   static {
      CASES.add(new Case("sin", 'd', (a, b) -> dbl(Math.sin(d(a)))));
      CASES.add(new Case("cos", 'd', (a, b) -> dbl(Math.cos(d(a)))));
      CASES.add(new Case("asin", 'd', (a, b) -> dbl(Math.asin(d(a)))));
      CASES.add(new Case("acos", 'd', (a, b) -> dbl(Math.acos(d(a)))));
      CASES.add(new Case("log", 'd', (a, b) -> dbl(Math.log(d(a)))));
      CASES.add(new Case("log1p", 'd', (a, b) -> dbl(Math.log1p(d(a)))));
      CASES.add(new Case("sqrt", 'd', (a, b) -> dbl(Math.sqrt(d(a)))));
      CASES.add(new Case("floor", 'd', (a, b) -> dbl(Math.floor(d(a)))));
      CASES.add(new Case("ceil", 'd', (a, b) -> dbl(Math.ceil(d(a)))));
      CASES.add(new Case("rint", 'd', (a, b) -> dbl(Math.rint(d(a)))));
      CASES.add(new Case("signum", 'd', (a, b) -> dbl(Math.signum(d(a)))));
      CASES.add(new Case("nextUp", 'd', (a, b) -> dbl(Math.nextUp(d(a)))));
      CASES.add(new Case("nextDown", 'd', (a, b) -> dbl(Math.nextDown(d(a)))));
      CASES.add(new Case("toRadians", 'd', (a, b) -> dbl(Math.toRadians(d(a)))));
      CASES.add(new Case("abs", 'd', (a, b) -> dbl(Math.abs(d(a)))));
      CASES.add(new Case("round", 'd', (a, b) -> Math.round(d(a))));
      CASES.add(new Case("atan2", 'D', (a, b) -> dbl(Math.atan2(d(a), d(b)))));
      CASES.add(new Case("pow", 'p', (a, b) -> dbl(Math.pow(d(a), d(b)))));
      CASES.add(new Case("pow_int", 'i', (a, b) -> dbl(Math.pow(d(a), (double)(int)b))));
      CASES.add(new Case("min", 'D', (a, b) -> dbl(Math.min(d(a), d(b)))));
      CASES.add(new Case("max", 'D', (a, b) -> dbl(Math.max(d(a), d(b)))));
      CASES.add(new Case("copySign", 'D', (a, b) -> dbl(Math.copySign(d(a), d(b)))));
      CASES.add(new Case("round_f", 'f', (a, b) -> Math.round(f(a))));
      CASES.add(new Case("signum_f", 'f', (a, b) -> flt(Math.signum(f(a)))));
      CASES.add(new Case("nextUp_f", 'f', (a, b) -> flt(Math.nextUp(f(a)))));
      CASES.add(new Case("nextDown_f", 'f', (a, b) -> flt(Math.nextDown(f(a)))));
      CASES.add(new Case("min_f", 'F', (a, b) -> flt(Math.min(f(a), f(b)))));
      CASES.add(new Case("max_f", 'F', (a, b) -> flt(Math.max(f(a), f(b)))));
      CASES.add(new Case("floorDiv_i", 'I', (a, b) -> Math.floorDiv((int)a, (int)(b == 0 ? 1 : b))));
      CASES.add(new Case("floorMod_i", 'I', (a, b) -> Math.floorMod((int)a, (int)(b == 0 ? 1 : b))));
      CASES.add(new Case("floorDiv_l", 'L', (a, b) -> Math.floorDiv(a, b == 0 ? 1 : b)));
      CASES.add(new Case("floorMod_l", 'L', (a, b) -> Math.floorMod(a, b == 0 ? 1 : b)));
      CASES.add(new Case("hypot", 'D', (a, b) -> dbl(Math.hypot(d(a), d(b)))));
      CASES.add(new Case("hypot_f", 'F', (a, b) -> flt((float)Math.hypot((double)f(a), (double)f(b)))));
      CASES.add(new Case("clamp_f", 'f', (a, b) -> flt(Math.clamp(f(a), -90.0F, 90.0F))));
      CASES.add(new Case("clamp_f_unit", 'f', (a, b) -> flt(Math.clamp(f(a), 0.0F, 1.0F))));
      CASES.add(new Case("clamp_d", 'd', (a, b) -> dbl(Math.clamp(d(a), 0.0, 32.0))));
      CASES.add(new Case("clamp_l", 'L', (a, b) -> Math.clamp(a, 0, (int)(b & 0x7fffffffL))));
      CASES.add(new Case("absExact", 'I', (a, b) -> exact(() -> Math.absExact((int)a))));
      CASES.add(new Case("negateExact", 'I', (a, b) -> exact(() -> Math.negateExact((int)a))));
      CASES.add(new Case("subtractExact", 'I', (a, b) -> exact(() -> Math.subtractExact((int)a, (int)b))));
      CASES.add(new Case("floorDivExact", 'I', (a, b) -> exact(() -> Math.floorDivExact((int)a, (int)b))));
      CASES.add(new Case("powExact", 'I', (a, b) -> exact(() -> Math.powExact((int)a, (int)(b & 0xff) % 40 - 5))));
      CASES.add(new Case("toIntExact", 'L', (a, b) -> exact(() -> Math.toIntExact(a))));
   }

   /**
    * An int result, or a marker (never a sign-extended int) when the call throws. The message is not part of the golden: once the
    * JIT compiled the call, HotSpot throws a pre-allocated exception without message (OmitStackTraceInFastThrow), so the same input
    * gives "integer overflow" in one run and null in another. The messages are asserted against the JDK sources in java.rs.
    */
   static long exact(java.util.function.IntSupplier call) {
      try {
         return call.getAsInt();
      } catch (ArithmeticException e) {
         return 0x7ff0L << 48;
      }
   }

   static final double[] EDGES = {
      0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5, 0.49999999999999994, -0.49999999999999994, 4503599627370496.0, 9007199254740992.0,
      9.223372036854775807E18, -9.223372036854775808E18, 2147483648.0, -2147483649.0, 1.0E300, -1.0E300, Double.MIN_VALUE, Double.MAX_VALUE,
      Double.MIN_NORMAL, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.NaN, 3.141592653589793, 1.5707963267948966, 6.283185307179586,
      10430.378350470453, 1.0E-9, 100.0, 255.0, 0.1, 0.9999999999999999, 1.0000000000000002,
   };

   static final int[] INT_EDGES = {0, 1, -1, 2, -2, 7, -7, 16, 255, 256, -256, 65535, Integer.MAX_VALUE, Integer.MIN_VALUE, Integer.MIN_VALUE + 1, Integer.MAX_VALUE - 1};

   static long state;

   static long next() {
      long z = (state += 0x9E3779B97F4A7C15L);
      z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
      z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
      return z ^ (z >>> 31);
   }

   static double unit() {
      return (next() >>> 11) * 0x1.0p-53;
   }

   static double mix() {
      long mode = next() & 7;
      long v = next();
      switch ((int)mode) {
         case 0: return d(v);
         case 1: return unit() * 2.0 - 1.0;
         case 2: return unit() * 200.0 - 100.0;
         case 3: {
            long e = (v >>> 52 & 0x7ff) % 121 - 60 + 1023;
            return d((v & 0x8000000000000000L) | e << 52 | (v & 0xfffffffffffffL));
         }
         case 4: return (double)((v >>> 40) & 0xfffff) * 1.5707963267948966 + (unit() - 0.5) * 2.0E-6;
         case 5: return unit() * 2.0E6 - 1.0E6;
         case 6: return (double)(int)((v >>> 32) % 129 - 64);
         default: return (double)(int)((v >>> 32) % 257 - 128) * 0.5;
      }
   }

   static long fnv(String s) {
      long h = 0xcbf29ce484222325L;
      for (byte b : s.getBytes(java.nio.charset.StandardCharsets.UTF_8)) {
         h = (h ^ (b & 0xff)) * 0x100000001b3L;
      }
      return h;
   }

   /** Inputs of index i; returns {a, b}. */
   static long[] input(char gen, int i) {
      int n = EDGES.length;
      switch (gen) {
         case 'd': return new long[] {i < n ? dbl(EDGES[i]) : dbl(mix()), 0};
         case 'D': {
            if (i < n * n) return new long[] {dbl(EDGES[i / n]), dbl(EDGES[i % n])};
            double x = mix();
            double y = mix();
            return new long[] {dbl(x), dbl(y)};
         }
         case 'p': {
            if (i < n * n) return new long[] {dbl(EDGES[i / n]), dbl(EDGES[i % n])};
            double x = mix();
            long k = (next() >>> 32) % 81 - 40;
            long half = next() % 3;
            long frac = next() % 4;
            double y = (double)(int)k * (half == 0 ? 0.5 : 1.0) + (frac == 0 ? unit() : 0.0);
            return new long[] {dbl(x), dbl(y)};
         }
         case 'i': {
            double x = i < n ? EDGES[i] : mix();
            long k = (next() >>> 32) % 81 - 40;
            return new long[] {dbl(x), (int)k};
         }
         case 'f': return new long[] {flt(i < n ? (float)EDGES[i] : (float)mix()) & 0xffffffffL, 0};
         case 'F': {
            if (i < n * n) return new long[] {flt((float)EDGES[i / n]) & 0xffffffffL, flt((float)EDGES[i % n]) & 0xffffffffL};
            double x = mix();
            double y = mix();
            return new long[] {flt((float)x) & 0xffffffffL, flt((float)y) & 0xffffffffL};
         }
         case 'I': {
            int m = INT_EDGES.length;
            if (i < m * m) return new long[] {INT_EDGES[i / m], INT_EDGES[i % m]};
            long a = next();
            long b = next();
            long shift = next() & 31;
            return new long[] {(int)a, (int)(b >>> shift)};
         }
         default: {
            int m = INT_EDGES.length;
            if (i < m * m) return new long[] {INT_EDGES[i / m], INT_EDGES[i % m]};
            long a = next();
            long b = next();
            long shift = next() & 63;
            return new long[] {a, b >> shift};
         }
      }
   }

   public static void main(String[] args) {
      boolean full = args.length == 2 && args[0].equals("full");
      StringBuilder out = new StringBuilder();
      if (!full) {
         out.append("# java.lang.Math golden, ").append(System.getProperty("java.vm.name")).append(' ').append(System.getProperty("java.version")).append('\n');
         // Mth.SIN: the table the sine/cosine lookups read, hashed like a case of 65536 inputs
         long h = 0xcbf29ce484222325L;
         out.append("case sin_table 65536 1024\n");
         for (int i = 0; i < 65536; i++) {
            h = (h ^ flt((float)Math.sin(i / 10430.378350470453))) * 0x100000001b3L;
            if ((i + 1) % 1024 == 0) {
               out.append(String.format("%016x%n", h));
               h = 0xcbf29ce484222325L;
            }
         }
      }
      for (Case c : CASES) {
         if (full && !c.name().equals(args[1])) continue;
         state = fnv(c.name());
         long h = 0xcbf29ce484222325L;
         if (!full) out.append("case ").append(c.name()).append(' ').append(COUNT).append(' ').append(BLOCK).append('\n');
         for (int i = 0; i < COUNT; i++) {
            long[] in = input(c.gen(), i);
            long r = c.fn().apply(in[0], in[1]);
            if (full) {
               out.append(String.format("%016x %016x %016x%n", in[0], in[1], r));
               if (out.length() > 1 << 16) {
                  System.out.print(out);
                  out.setLength(0);
               }
            } else {
               h = (h ^ r) * 0x100000001b3L;
               if ((i + 1) % BLOCK == 0) {
                  out.append(String.format("%016x%n", h));
                  h = 0xcbf29ce484222325L;
               }
            }
         }
      }
      System.out.print(out);
   }
}
