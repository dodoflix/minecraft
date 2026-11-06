import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import net.minecraft.util.RandomSource;
import net.minecraft.util.valueproviders.FloatProvider;
import net.minecraft.util.valueproviders.FloatProviders;
import net.minecraft.util.valueproviders.IntProvider;
import net.minecraft.util.valueproviders.IntProviders;
import net.minecraft.world.level.levelgen.WorldGenerationContext;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.heightproviders.HeightProvider;

/**
 * Golden generator for IntProvider / FloatProvider / HeightProvider codecs and sampling.
 * Usage: scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/providers.txt.gz crates/mc-worldgen/tests/oracle/ProviderOracle.java crates/mc-worldgen/tests/fixtures/providers.txt.gz
 * Output: crates/mc-worldgen/tests/fixtures/providers.txt.gz
 *
 * Line: KIND \t json \t OK|ERR \t [min \t max \t] sample-lines...
 *   KIND: int | int_nn | int_pos | int_r:min:max | float | float_min:v | float_r:min:max | height
 *   After OK: "min max" (ints; floats as raw hex bits; height has none) then one field per RNG spec,
 *   each "<tag>=v,v,v,...,L<next long>" (floats as hex bits, ints decimal, X = vanilla threw).
 *   RNG specs: legacy(0), legacy(12345), xoroshiro(42); height additionally per context (overworld, nether, custom).
 */
public class ProviderOracle {
   static final long[][] SPECS = {{0, 0}, {0, 12345}, {1, 42}};
   static final int SAMPLES = 40;
   static final int[][] CONTEXTS = {{-64, 384, 63}, {0, 128, 32}, {-32, 112, 10}};
   static final Random GEN = new Random(20260101L);
   static PrintWriter out;

   static RandomSource rng(long[] spec) {
      return spec[0] == 0 ? new LegacyRandomSource(spec[1]) : new XoroshiroRandomSource(spec[1]);
   }

   static WorldGenerationContext ctx(int[] c) throws Exception {
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      uf.setAccessible(true);
      sun.misc.Unsafe unsafe = (sun.misc.Unsafe)uf.get(null);
      WorldGenerationContext context = (WorldGenerationContext)unsafe.allocateInstance(WorldGenerationContext.class);
      String[] names = {"minY", "height", "seaLevel"};
      for (int i = 0; i < 3; i++) {
         Field f = WorldGenerationContext.class.getDeclaredField(names[i]);
         f.setAccessible(true);
         f.setInt(context, c[i]);
      }
      return context;
   }

   static String bits(float f) {
      return String.format(java.util.Locale.ROOT, "%08x", Float.floatToRawIntBits(f));
   }

   interface Sampler {
      String sample(RandomSource random) throws Exception;
   }

   static String run(Sampler sampler, long[] spec) {
      RandomSource random = rng(spec);
      StringBuilder sb = new StringBuilder();
      try {
         for (int i = 0; i < SAMPLES; i++) {
            sb.append(sampler.sample(random)).append(',');
         }
         sb.append('L').append(random.nextLong());
      } catch (Throwable t) {
         sb.append('X');
      }
      return sb.toString();
   }

   static void emit(String kind, String json, Codec<?> codec) throws Exception {
      JsonElement element = JsonParser.parseString(json);
      DataResult<?> result;
      try {
         result = codec.parse(JsonOps.INSTANCE, element);
      } catch (Throwable t) {
         out.println(kind + "\t" + json + "\tERR");
         return;
      }
      if (result.result().isEmpty()) {
         out.println(kind + "\t" + json + "\tERR");
         return;
      }
      Object value = result.result().get();
      StringBuilder line = new StringBuilder(kind + "\t" + json + "\tOK");
      if (value instanceof IntProvider p) {
         line.append('\t').append(p.minInclusive()).append(' ').append(p.maxInclusive());
         for (long[] spec : SPECS) {
            line.append('\t').append("i").append(spec[0]).append('_').append(spec[1]).append('=').append(run(r -> Integer.toString(p.sample(r)), spec));
         }
      } else if (value instanceof FloatProvider p) {
         line.append('\t').append(bits(p.min())).append(' ').append(bits(p.max()));
         for (long[] spec : SPECS) {
            line.append('\t').append("f").append(spec[0]).append('_').append(spec[1]).append('=').append(run(r -> bits(p.sample(r)), spec));
         }
      } else if (value instanceof HeightProvider p) {
         for (int[] c : CONTEXTS) {
            WorldGenerationContext context = ctx(c);
            for (long[] spec : SPECS) {
               line.append('\t').append("h").append(c[0]).append('_').append(spec[0]).append('_').append(spec[1]).append('=')
                  .append(run(r -> Integer.toString(p.sample(r, context)), spec));
            }
         }
      }
      out.println(line);
   }

   // ---- random corpus ------------------------------------------------------------------------------

   static String pick(String... options) {
      return options[GEN.nextInt(options.length)];
   }

   static String num(String... options) {
      return pick(options);
   }

   static String intNum() {
      return pick("-20", "-8", "-3", "-1", "0", "1", "2", "3", "5", "7", "10", "16", "40", "100");
   }

   static String floatNum() {
      return pick("-2.5", "-1.0", "-0.4", "-0.125", "0.0", "0.1", "0.25", "0.5", "0.75", "1.0", "1.4", "2.0", "3.0", "6.0", "12.5");
   }

   static String genInt(int depth) {
      int t = GEN.nextInt(depth > 2 ? 6 : 10);
      switch (t) {
         case 0:
            return intNum();
         case 1:
            return "{\"type\":\"minecraft:constant\",\"value\":" + intNum() + "}";
         case 2:
            return "{\"type\":\"" + pick("uniform", "minecraft:uniform") + "\",\"min_inclusive\":" + intNum() + ",\"max_inclusive\":" + intNum() + "}";
         case 3:
            return "{\"type\":\"biased_to_bottom\",\"min_inclusive\":" + intNum() + ",\"max_inclusive\":" + intNum() + "}";
         case 4:
            return "{\"type\":\"very_biased_to_bottom\",\"min_inclusive\":" + intNum() + ",\"max_inclusive\":" + intNum() + "}";
         case 5:
            return "{\"type\":\"trapezoid\",\"min\":" + intNum() + ",\"max\":" + intNum() + ",\"plateau\":" + pick("0", "1", "2", "4", "8", "20", "-3") + "}";
         case 6:
            return "{\"type\":\"clamped_normal\",\"mean\":" + floatNum() + ",\"deviation\":" + floatNum() + ",\"min_inclusive\":" + intNum() + ",\"max_inclusive\":" + intNum() + "}";
         case 7:
            return "{\"type\":\"clamped\",\"source\":" + genInt(depth + 1) + ",\"min_inclusive\":" + intNum() + ",\"max_inclusive\":" + intNum() + "}";
         default: {
            int n = 1 + GEN.nextInt(4);
            StringBuilder sb = new StringBuilder("{\"type\":\"weighted_list\",\"distribution\":[");
            for (int i = 0; i < n; i++) {
               if (i > 0) sb.append(',');
               sb.append("{\"data\":").append(genInt(depth + 1)).append(",\"weight\":").append(pick("0", "1", "2", "5", "40", "100")).append('}');
            }
            return sb.append("]}").toString();
         }
      }
   }

   static String genFloat(int depth) {
      int t = GEN.nextInt(depth > 2 ? 3 : 6);
      switch (t) {
         case 0:
            return floatNum();
         case 1:
            return "{\"type\":\"constant\",\"value\":" + floatNum() + "}";
         case 2:
            return "{\"type\":\"uniform\",\"min_inclusive\":" + floatNum() + ",\"max_exclusive\":" + floatNum() + "}";
         case 3:
            return "{\"type\":\"clamped_normal\",\"mean\":" + floatNum() + ",\"deviation\":" + floatNum() + ",\"min\":" + floatNum() + ",\"max\":" + floatNum() + "}";
         default:
            return "{\"type\":\"trapezoid\",\"min\":" + floatNum() + ",\"max\":" + floatNum() + ",\"plateau\":" + floatNum() + "}";
      }
   }

   static String anchor() {
      String key = pick("absolute", "above_bottom", "below_top", "relative_to_sea_level");
      return "{\"" + key + "\":" + pick("-64", "-30", "-8", "-1", "0", "1", "5", "20", "64", "100", "250") + "}";
   }

   static String genHeight(int depth) {
      int t = GEN.nextInt(depth > 1 ? 7 : 9);
      switch (t) {
         case 0:
            return anchor();
         case 1:
            return "{\"type\":\"constant\",\"value\":" + anchor() + "}";
         case 2:
         case 3:
            return "{\"type\":\"uniform\",\"min_inclusive\":" + anchor() + ",\"max_inclusive\":" + anchor() + "}";
         case 4:
            return "{\"type\":\"biased_to_bottom\",\"min_inclusive\":" + anchor() + ",\"max_inclusive\":" + anchor() + pick("", ",\"inner\":1", ",\"inner\":4", ",\"inner\":30", ",\"inner\":0") + "}";
         case 5:
            return "{\"type\":\"very_biased_to_bottom\",\"min_inclusive\":" + anchor() + ",\"max_inclusive\":" + anchor() + pick("", ",\"inner\":1", ",\"inner\":4", ",\"inner\":30") + "}";
         case 6:
            return "{\"type\":\"trapezoid\",\"min_inclusive\":" + anchor() + ",\"max_inclusive\":" + anchor() + pick("", ",\"plateau\":0", ",\"plateau\":5", ",\"plateau\":100", ",\"plateau\":-4") + "}";
         default: {
            int n = 1 + GEN.nextInt(3);
            StringBuilder sb = new StringBuilder("{\"type\":\"weighted_list\",\"distribution\":[");
            for (int i = 0; i < n; i++) {
               if (i > 0) sb.append(',');
               sb.append("{\"data\":").append(genHeight(depth + 1)).append(",\"weight\":").append(pick("0", "1", "3", "70")).append('}');
            }
            return sb.append("]}").toString();
         }
      }
   }

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      out = new PrintWriter(args[0]);

      List<String> edgeInt = List.of(
         "5", "0", "-7", "5.7", "-5.7", "1e3", "1e10", "3000000000", "true", "\"5\"", "null", "[]", "{}", "[1]",
         "{\"type\":\"constant\",\"value\":3}", "{\"type\":\"constant\"}", "{\"type\":\"constant\",\"value\":true}",
         "{\"type\":\"constant\",\"value\":3,\"extra\":1}", "{\"type\":\"nope\",\"value\":3}", "{\"type\":5,\"value\":3}", "{\"value\":3}",
         "{\"type\":\"other:constant\",\"value\":3}", "{\"type\":\"Minecraft:constant\",\"value\":3}", "{\"type\":\"minecraft:constant\",\"value\":3.9}",
         "{\"type\":\"uniform\",\"min_inclusive\":5,\"max_inclusive\":4}", "{\"type\":\"uniform\",\"min_inclusive\":5,\"max_inclusive\":5}",
         "{\"type\":\"uniform\",\"min_inclusive\":5}", "{\"type\":\"uniform\",\"min_inclusive\":\"5\",\"max_inclusive\":6}",
         "{\"type\":\"uniform\",\"min_inclusive\":-2147483648,\"max_inclusive\":2147483647}",
         "{\"type\":\"biased_to_bottom\",\"min_inclusive\":-2147483648,\"max_inclusive\":2147483647}",
         "{\"type\":\"very_biased_to_bottom\",\"min_inclusive\":0,\"max_inclusive\":0}",
         "{\"type\":\"trapezoid\",\"min\":0,\"max\":10,\"plateau\":11}", "{\"type\":\"trapezoid\",\"min\":0,\"max\":10,\"plateau\":10}",
         "{\"type\":\"trapezoid\",\"min\":-4,\"max\":4,\"plateau\":0}", "{\"type\":\"trapezoid\",\"min\":-4,\"max\":3,\"plateau\":0}",
         "{\"type\":\"trapezoid\",\"min\":3,\"max\":1,\"plateau\":0}", "{\"type\":\"trapezoid\",\"min\":0,\"max\":10,\"plateau\":-2}",
         "{\"type\":\"clamped_normal\",\"mean\":3.5,\"deviation\":2.0,\"min_inclusive\":0,\"max_inclusive\":6}",
         "{\"type\":\"clamped_normal\",\"mean\":3.5,\"deviation\":2.0,\"min_inclusive\":6,\"max_inclusive\":0}",
         "{\"type\":\"clamped\",\"source\":{\"type\":\"uniform\",\"min_inclusive\":-10,\"max_inclusive\":10},\"min_inclusive\":-3,\"max_inclusive\":4}",
         "{\"type\":\"clamped\",\"source\":3,\"min_inclusive\":4,\"max_inclusive\":2}", "{\"type\":\"clamped\",\"min_inclusive\":1,\"max_inclusive\":2}",
         "{\"type\":\"weighted_list\",\"distribution\":[]}", "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":3,\"weight\":0}]}",
         "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":3,\"weight\":-1}]}", "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":3}]}",
         "{\"type\":\"weighted_list\",\"distribution\":[{\"weight\":3}]}", "{\"type\":\"weighted_list\",\"distribution\":{}}",
         "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":3,\"weight\":1},{\"data\":{\"type\":\"uniform\",\"min_inclusive\":10,\"max_inclusive\":20},\"weight\":3}]}",
         "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":1,\"weight\":70},{\"data\":2,\"weight\":30},{\"data\":9,\"weight\":0}]}",
         "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":1,\"weight\":2147483647},{\"data\":2,\"weight\":1}]}"
      );
      for (String json : edgeInt) {
         emit("int", json, IntProviders.CODEC);
         emit("int_nn", json, IntProviders.NON_NEGATIVE_CODEC);
         emit("int_pos", json, IntProviders.POSITIVE_CODEC);
         emit("int_r:-5:20", json, IntProviders.codec(-5, 20));
      }
      for (int i = 0; i < 400; i++) {
         String json = genInt(0);
         emit("int", json, IntProviders.CODEC);
         if (i % 4 == 0) emit("int_nn", json, IntProviders.NON_NEGATIVE_CODEC);
         if (i % 4 == 1) emit("int_pos", json, IntProviders.POSITIVE_CODEC);
         if (i % 4 == 2) emit("int_r:-5:20", json, IntProviders.codec(-5, 20));
      }

      List<String> edgeFloat = List.of(
         "5", "0", "-0.0", "0.5", "-7.25", "1e3", "1e39", "true", "\"5\"", "null", "[]", "{}",
         "{\"type\":\"constant\",\"value\":3.5}", "{\"type\":\"constant\",\"value\":-0.0}", "{\"type\":\"constant\"}",
         "{\"type\":\"uniform\",\"min_inclusive\":1.0,\"max_exclusive\":1.0}", "{\"type\":\"uniform\",\"min_inclusive\":1.0,\"max_exclusive\":0.5}",
         "{\"type\":\"uniform\",\"min_inclusive\":0.1,\"max_exclusive\":0.9}", "{\"type\":\"uniform\",\"min_inclusive\":-1,\"max_exclusive\":2}",
         "{\"type\":\"trapezoid\",\"min\":0.0,\"max\":6.0,\"plateau\":2.0}", "{\"type\":\"trapezoid\",\"min\":0.0,\"max\":6.0,\"plateau\":6.0}",
         "{\"type\":\"trapezoid\",\"min\":0.0,\"max\":6.0,\"plateau\":6.5}", "{\"type\":\"trapezoid\",\"min\":2.0,\"max\":1.0,\"plateau\":0}",
         "{\"type\":\"trapezoid\",\"min\":0.0,\"max\":6.0,\"plateau\":-1.0}",
         "{\"type\":\"clamped_normal\",\"mean\":0.3,\"deviation\":0.9,\"min\":-1.0,\"max\":1.0}",
         "{\"type\":\"clamped_normal\",\"mean\":0.3,\"deviation\":0.9,\"min\":1.0,\"max\":-1.0}",
         "{\"type\":\"clamped_normal\",\"mean\":0.3,\"deviation\":0.9,\"min\":1.0,\"max\":1.0}",
         "{\"type\":\"weighted_list\",\"distribution\":[]}", "{\"type\":\"nope\"}", "{\"type\":\"minecraft:uniform\",\"min_inclusive\":0,\"max_exclusive\":3}"
      );
      for (String json : edgeFloat) {
         emit("float", json, FloatProviders.CODEC);
         emit("float_min:0.0", json, FloatProviders.codec(0.0F));
         emit("float_r:-1.0:1.0", json, FloatProviders.codec(-1.0F, 1.0F));
      }
      for (int i = 0; i < 300; i++) {
         String json = genFloat(0);
         emit("float", json, FloatProviders.CODEC);
         if (i % 3 == 0) emit("float_min:0.0", json, FloatProviders.codec(0.0F));
         if (i % 3 == 1) emit("float_r:-1.0:1.0", json, FloatProviders.codec(-1.0F, 1.0F));
      }

      List<String> edgeHeight = List.of(
         "{\"absolute\":5}", "{\"above_bottom\":8}", "{\"below_top\":1}", "{\"relative_to_sea_level\":-10}",
         "{\"absolute\":5,\"above_bottom\":8}", "{\"absolute\":2032}", "{\"absolute\":2031}", "{\"absolute\":-2032}", "{\"absolute\":-2033}", "{\"absolute\":\"5\"}",
         "5", "\"5\"", "null", "[]", "{}", "{\"type\":\"constant\",\"value\":{\"absolute\":9}}", "{\"type\":\"constant\"}",
         "{\"type\":\"uniform\",\"min_inclusive\":{\"absolute\":9},\"max_inclusive\":{\"absolute\":3}}",
         "{\"type\":\"uniform\",\"min_inclusive\":{\"absolute\":9}}", "{\"type\":\"nope\"}", "{\"type\":\"minecraft:uniform\",\"min_inclusive\":{\"above_bottom\":0},\"max_inclusive\":{\"below_top\":0}}",
         "{\"type\":\"trapezoid\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":10},\"plateau\":3}",
         "{\"type\":\"trapezoid\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":10},\"plateau\":\"3\"}",
         "{\"type\":\"biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":10},\"inner\":0}",
         "{\"type\":\"biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":10},\"inner\":3}",
         "{\"type\":\"very_biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":10},\"inner\":3}",
         "{\"type\":\"very_biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":2},\"inner\":3}",
         "{\"type\":\"weighted_list\",\"distribution\":[]}", "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":{\"absolute\":4},\"weight\":1}]}",
         "{\"type\":\"weighted_list\",\"distribution\":[{\"data\":{\"type\":\"uniform\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":9}},\"weight\":1},{\"data\":{\"above_bottom\":3},\"weight\":2}]}"
      );
      for (String json : edgeHeight) {
         emit("height", json, HeightProvider.CODEC);
      }
      for (int i = 0; i < 400; i++) {
         emit("height", genHeight(0), HeightProvider.CODEC);
      }
      out.close();
      System.exit(0);
   }
}
