import java.io.PrintWriter;
import java.util.List;
import net.minecraft.util.valueproviders.FloatProviders;
import net.minecraft.util.valueproviders.IntProviders;
import net.minecraft.world.level.levelgen.heightproviders.HeightProvider;

/**
 * Golden generator for the provider codecs on numbers `serde_json` cannot hold (beyond double, integers beyond 64 bits) and on
 * an object that spells the key of the stand-in for such a number; reuses the line format of {@link ProviderOracle}.
 * Usage: scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/provider_numbers.txt.gz ProviderNumbersOracle crates/mc-worldgen/tests/fixtures/provider_numbers.txt.gz
 * Output: crates/mc-worldgen/tests/fixtures/provider_numbers.txt.gz, lines in the format of ProviderOracle
 * after `javac -cp "$(scripts/vanilla-cp.sh)" -d <dir> ProviderOracle.java ProviderNumbersOracle.java` with MC_EXTRA_CP=<dir>
 */
public class ProviderNumbersOracle {
   static String o(String type, String... fields) {
      StringBuilder sb = new StringBuilder("{\"type\":\"minecraft:" + type + "\"");
      for (int i = 0; i < fields.length; i += 2) sb.append(",\"").append(fields[i]).append("\":").append(fields[i + 1]);
      return sb.append('}').toString();
   }

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      ProviderOracle.out = new PrintWriter(args[0]);
      String lazy = "{\"\\u0000lazy_number\":\"1e400\"}";
      String bigInt = "123456789012345678901";
      List<String> ints = List.of(
         "1e400", "-1e400", "1e-400", "1e10000", "-1e10000", bigInt, "-" + bigInt, "1e19", "9223372036854775808", "4294967297.5", "0.99999999999999999999",
         lazy, "[1e400]", o("constant", "value", "1e400"), o("constant", "value", "1e10000"), o("constant", "value", lazy),
         o("uniform", "min_inclusive", "-1e400", "max_inclusive", "1e400"), o("uniform", "min_inclusive", "1e400", "max_inclusive", "5"),
         o("uniform", "min_inclusive", "0", "max_inclusive", "1e10000"), o("uniform", "min_inclusive", bigInt, "max_inclusive", bigInt),
         o("biased_to_bottom", "min_inclusive", "0", "max_inclusive", "1e400"), o("clamped", "source", "1e400", "min_inclusive", "0", "max_inclusive", "5"),
         o("clamped", "source", "3", "min_inclusive", "-1e400", "max_inclusive", "1e400"),
         o("clamped_normal", "mean", "1e400", "deviation", "1e400", "min_inclusive", "0", "max_inclusive", "5"),
         o("weighted_list", "distribution", "[{\"data\":1,\"weight\":1e400},{\"data\":2,\"weight\":1}]"),
         o("weighted_list", "distribution", "[{\"data\":1e400,\"weight\":1}]"), o("weighted_list", "distribution", "[{\"data\":" + lazy + ",\"weight\":1}]"),
         o("constant", "value", "5", "extra", "1e400"), "{\"type\":1e400}", "{\"type\":" + lazy + "}");
      for (String json : ints) {
         ProviderOracle.emit("int", json, IntProviders.CODEC);
         ProviderOracle.emit("int_nn", json, IntProviders.NON_NEGATIVE_CODEC);
         ProviderOracle.emit("int_pos", json, IntProviders.POSITIVE_CODEC);
         ProviderOracle.emit("int_r:-5:20", json, IntProviders.codec(-5, 20));
      }
      List<String> floats = List.of(
         "1e400", "-1e400", "1e-400", "1e10000", "-1e10000", bigInt, "1e39", "-1e39", "0.99999999999999999999", lazy, "[1e400]",
         o("constant", "value", "1e400"), o("constant", "value", "-1e400"), o("constant", "value", "1e10000"), o("constant", "value", lazy),
         o("uniform", "min_inclusive", "-1e400", "max_exclusive", "1e400"), o("uniform", "min_inclusive", "0", "max_exclusive", "1e400"),
         o("uniform", "min_inclusive", "1e400", "max_exclusive", "1e400"), o("uniform", "min_inclusive", "1e-400", "max_exclusive", "1"),
         o("clamped_normal", "mean", "1e400", "deviation", "1", "min", "-1", "max", "1"), o("clamped_normal", "mean", "0", "deviation", "1e400", "min", "-1", "max", "1"),
         o("clamped_normal", "mean", "0", "deviation", "1", "min", "-1e400", "max", "1e400"), o("trapezoid", "min", "0", "max", "6", "plateau", "1e400"),
         o("trapezoid", "min", "-1e400", "max", "1e400", "plateau", "0"), o("trapezoid", "min", "0", "max", "1e400", "plateau", "1e400"),
         o("weighted_list", "distribution", "[]"), "{\"type\":1e400}", "{\"type\":" + lazy + "}");
      for (String json : floats) {
         ProviderOracle.emit("float", json, FloatProviders.CODEC);
         ProviderOracle.emit("float_min:0.0", json, FloatProviders.codec(0.0F));
         ProviderOracle.emit("float_r:-1.0:1.0", json, FloatProviders.codec(-1.0F, 1.0F));
      }
      List<String> heights = List.of(
         "1e400", "-1e400", "1e10000", bigInt, lazy, "[1e400]", "{\"absolute\":1e400}", "{\"absolute\":-1e400}", "{\"absolute\":1e10000}", "{\"absolute\":1e400,\"above_bottom\":1e400}",
         "{\"above_bottom\":1e400}", "{\"below_top\":-1e400}", "{\"relative_to_sea_level\":1e400}", "{\"absolute\":" + bigInt + "}", "{\"absolute\":" + lazy + "}", "{\"absolute\":4294967297}",
         o("constant", "value", "{\"absolute\":1e400}"), o("constant", "value", "1e400"), o("constant", "value", lazy),
         o("uniform", "min_inclusive", "{\"absolute\":1e400}", "max_inclusive", "{\"absolute\":5}"), o("uniform", "min_inclusive", "{\"above_bottom\":0}", "max_inclusive", "{\"below_top\":1e400}"),
         o("uniform", "min_inclusive", "1e400", "max_inclusive", "{\"absolute\":5}"),
         o("biased_to_bottom", "min_inclusive", "{\"absolute\":0}", "max_inclusive", "{\"absolute\":10}", "inner", "1e400"),
         o("biased_to_bottom", "min_inclusive", "{\"absolute\":0}", "max_inclusive", "{\"absolute\":10}", "inner", "1e10000"),
         o("very_biased_to_bottom", "min_inclusive", "{\"absolute\":0}", "max_inclusive", "{\"absolute\":10}", "inner", "1e400"),
         o("trapezoid", "min_inclusive", "{\"absolute\":0}", "max_inclusive", "{\"absolute\":10}", "plateau", "1e400"),
         o("trapezoid", "min_inclusive", "{\"absolute\":0}", "max_inclusive", "{\"absolute\":10}", "plateau", "-1e400"),
         o("weighted_list", "distribution", "[{\"data\":{\"absolute\":4},\"weight\":1e400}]"), o("weighted_list", "distribution", "[{\"data\":{\"absolute\":1e400},\"weight\":1}]"),
         o("weighted_list", "distribution", "[{\"data\":1e400,\"weight\":1}]"), "{\"type\":1e400}", "{\"type\":" + lazy + "}");
      for (String json : heights) ProviderOracle.emit("height", json, HeightProvider.CODEC);
      ProviderOracle.out.close();
      System.exit(0);
   }
}
