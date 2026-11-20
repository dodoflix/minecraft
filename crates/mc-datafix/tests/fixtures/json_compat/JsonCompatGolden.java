import com.mojang.serialization.Dynamic;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.Random;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.StringTag;
import net.minecraft.util.GsonHelper;
import net.minecraft.util.LenientJsonParser;
import net.minecraft.util.StrictJsonParser;
import net.minecraft.util.datafix.LegacyComponentDataFixUtils;
import com.google.gson.JsonElement;
import com.google.gson.internal.LazilyParsedNumber;

/**
 * Golden generator for the Gson-compatible JSON stack the data fixes use (mc_core::gson, mc_datafix::legacy_component_data_fix_utils).
 * Line: op TAB escaped-input TAB result. `esc` writes UTF-16 code units outside printable ASCII as \\uXXXX.
 * Usage: crates/mc-datafix/tests/fixtures/json_compat/gen.sh
 * Output: crates/mc-datafix/tests/fixtures/json_compat.tsv
 */
public class JsonCompatGolden {
   static final StringBuilder OUT = new StringBuilder();

   static String esc(String s) {
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < s.length(); i++) {
         char c = s.charAt(i);
         if (c == '\\') sb.append("\\\\");
         else if (c == '\t') sb.append("\\t");
         else if (c == '\n') sb.append("\\n");
         else if (c == '\r') sb.append("\\r");
         else if (c < 0x20 || c > 0x7e) sb.append(String.format("\\u%04x", (int) c));
         else sb.append(c);
      }
      return sb.toString();
   }

   static void line(String op, String input, String result) {
      OUT.append(op).append('\t').append(esc(input)).append('\t').append(esc(result)).append('\n');
   }

   static String err(Throwable t) {
      return "ERR:" + t.getClass().getSimpleName() + ":" + t.getMessage();
   }

   static String stable(JsonElement e) {
      return GsonHelper.toStableString(e);
   }

   static void all(String s) {
      try {
         line("lenient_stable", s, stable(LenientJsonParser.parse(s)));
      } catch (Throwable t) {
         line("lenient_stable", s, err(t));
      }
      try {
         line("strict_stable", s, stable(StrictJsonParser.parse(s)));
      } catch (Throwable t) {
         line("strict_stable", s, err(t));
      }
      try {
         line("rewrite_from_lenient", s, LegacyComponentDataFixUtils.rewriteFromLenient(s));
      } catch (Throwable t) {
         line("rewrite_from_lenient", s, err(t));
      }
      try {
         Dynamic<?> d = new Dynamic<>(NbtOps.INSTANCE, StringTag.valueOf(s));
         line("strict_valid", s, Boolean.toString(LegacyComponentDataFixUtils.isStrictlyValidJson(d)));
      } catch (Throwable t) {
         line("strict_valid", s, err(t));
      }
      try {
         Optional<String> r = LegacyComponentDataFixUtils.extractTranslationString(s);
         line("extract_translation", s, r.map(v -> "some:" + v).orElse("none"));
      } catch (Throwable t) {
         line("extract_translation", s, err(t));
      }
      line("text_json", s, LegacyComponentDataFixUtils.createTextComponentJson(s));
      line("translatable_json", s, LegacyComponentDataFixUtils.createTranslatableComponentJson(s));
   }

   static String nest(String open, String close, int depth, String inner) {
      return open.repeat(depth) + inner + close.repeat(depth);
   }

   public static void main(String[] args) throws Exception {
      List<String> in = new ArrayList<>();
      // numbers and literals
      for (String s : new String[] {"1", "1.0", "1e3", "1E3", "1E+2", "0.1", "-0", "-0.0", "1.5e-10", "123456789012345678901234567890", "1e400",
         "0x10", "01", "+1", ".5", "5.", "NaN", "Infinity", "-Infinity", "1_000", "-", "--1", "1e", "1e+", "2147483648", "-2147483649",
         "9223372036854775808", "0.0000001", "100000000000000000000.5", "null", "NULL", "nul", "true", "false", "True", "\"true\"",
         "", " ", "  \n ", "\t", "\u00a0", "\ufeff", "\ufeff{\"a\":1}", "1".repeat(1100), "1".repeat(1023), "1".repeat(1024)}) in.add(s);
      // strings
      for (String s : new String[] {"\"\"", "\"a\"", "\"a", "a\"", "\"", "'a'", "'a", "\"a\\nb\"", "\"\\u00e9\"", "\"\\u0041\"", "\"\\ud83d\\ude00\"",
         "\"\\x41\"", "\"a\\'b\"", "\"tab\there\"", "\"new\nline\"", "\"\\/\"", "\"\u00e9\"", "\"\u00a7cred\"", "\"<>&='\"", "\"\u2028\u2029\"",
         "\"\\u0001\"", "\"\u007f\u0080\"", "\"\\q\"", "\"\\u12\"", "\"\\uZZZZ\"", "\"a\"b\"", "\"a\" \"b\"", "\"a\"x", "\"a\"\"b\"",
         "'a\\'b'", "\"\ud83d\ude00\"", "\"\\\\\"", "\"\\\"\"", "\"unterminated\\", "\"\\b\\f\\n\\r\\t\"", "unquoted", "unquoted text",
         "a b c", "plain \"quoted\" text", "[not json]", "{not json}", "{\"half"}) in.add(s);
      // objects
      for (String s : new String[] {"{}", "{ }", "{\"text\":\"hi\"}", "{text:hi}", "{'text':'hi'}", "{\"b\":1,\"a\":2}", "{\"a\":1,\"a\":2}",
         "{\"a\":1,}", "{,}", "{\"a\" 1}", "{\"a\"=1}", "{\"a\"=>1}", "{\"a\":1;\"b\":2}", "{\"a\":[1,2,{\"b\":null}]}",
         "{\"translate\":\"chat.type.text\",\"with\":[\"a\",\"b\"]}", "{\"translate\":\"x\"}", "{\"translate\":1}", "{\"translate\":1.50}",
         "{\"translate\":true}", "{\"translate\":null}", "{\"translate\":[\"x\"]}", "{\"translate\":{}}", "{\"translate\":\"\"}",
         "{'translate':'k'}", "{translate:k}", "{\"translate\":\"a\",\"translate\":\"b\"}", "{\"extra\":[{\"text\":\"a\"}],\"text\":\"\"}",
         "{\"a\":01}", "{\"a\":NaN}", "{\"a\":Infinity}", "{\"a\":-Infinity}", "{\"a\":0x1}", "{\"a\":.5}", "{\"a\":1.}", "{\"a\":+1}",
         "{/*c*/\"a\":1}", "{\"a\":1}//c", "{\"a\":1}#c", "# c\n{}", "// c\n{}", "/* c */{}", "{\"a\":1} x", "{\"a\":1}{\"b\":2}", "{\"a\":1}}",
         "{a-b:1}", "{a b:1}", "{\"a\":'x'}", "{\"a\":x y}", "{\"a\":unquoted}", "{\"a\":\"unterminated}", "{\"a\":\"\\u12\"}", "{\"a\":\"\\uZZZZ\"}",
         "{\"a\":\"\\q\"}", "{\"\":1}", "{\"A\":1,\"a\":2}", "{\"n\":1.0,\"m\":1e3,\"o\":0.1,\"p\":-0,\"q\":1E+2,\"r\":123456789012345678901234567890}",
         "{\"a\":null,\"b\":[null]}", "{\"\u00e9\":1,\"z\":2,\"a\":3,\"\ud83d\ude00\":4,\"\uffff\":5,\"\ue000\":6,\"\u00ff\":7,\"\u0100\":8}",
         "{\"b\":{\"d\":1,\"c\":2},\"a\":[{\"y\":1,\"x\":2}]}", "{\"text\":\"<b>&amp;='\"}", "{\"text\":\"\\u2028\\u2029\"}",
         "{\"a\":true,\"b\":false}", "{\"a\":\"true\"}", "{\"text\":\"a\",\"color\":\"red\",\"bold\":true,\"extra\":[\"b\"]}",
         "{\"clickEvent\":{\"action\":\"run_command\",\"value\":\"/x\"},\"hoverEvent\":{\"action\":\"show_text\",\"value\":\"y\"},\"text\":\"t\"}",
         "{\"text\":\"\\\"quoted\\\"\"}", "{\"\\u0061\":1}", "{\"a\":1 \"b\":2}", "{\"a\":[1 2]}", "{\"a\"}", "{:1}", "{\"a\":}", "{\"a\":1,,}",
         "{\"a\":1\n}", "{\n\t\"a\"\t:\t1\n}", "{\"a\" : 1 , \"b\" : 2}"}) in.add(s);
      // arrays
      for (String s : new String[] {"[]", "[ ]", "[1,2]", "[1,2,]", "[,]", "[,1]", "[1,,2]", "[null]", "[1 2]", "[1;2]", "[\"a\" \"b\"]", "[\"a\",\"b\"]",
         "[1,2] x", "[1,2]]", "[abc}", "{abc]", "[{\"a\":1},{\"b\":2}]", "[[1],[2,[3]]]", "[\"a\",{\"text\":\"b\"}]", "[1.0,1e3,0.1]", "[true,false,null]",
         "[\"\"]", "[\"a\"", "[1,", "[", "]", "{", "}", "[]x", "x[]", "[1]\n", " [1]", "[\u00e9]", "['a','b']", "[a,b]", "[a b]"}) in.add(s);
      // nesting limit 255 (JsonReader.DEFAULT_NESTING_LIMIT)
      for (int d : new int[] {1, 2, 254, 255, 256, 257, 300}) {
         in.add(nest("[", "]", d, ""));
         in.add(nest("[", "]", d, "1"));
         in.add(nest("{\"a\":", "}", d, "1"));
      }
      // many keys in shuffled order
      Random r = new Random(7);
      List<String> keys = new ArrayList<>();
      for (int i = 0; i < 60; i++) keys.add("k" + i);
      java.util.Collections.shuffle(keys, r);
      StringBuilder sb = new StringBuilder("{");
      for (int i = 0; i < keys.size(); i++) {
         if (i > 0) sb.append(',');
         sb.append('"').append(keys.get(i)).append("\":").append(i);
      }
      in.add(sb.append('}').toString());
      // text component payloads as stored in pre-1.21.5 worlds
      for (String s : new String[] {"{\"text\":\"\"}", "{\"translate\":\"block.minecraft.chest\"}", "\"just a string\"", "[{\"text\":\"a\"},{\"text\":\"b\"}]",
         "{\"text\":\"\u00a7aGreen\"}", "\u00a7aGreen name", "Plain \"name\" with <html> & stuff", "{\"extra\":[\"a\"],\"text\":\"\"}", "\"\"", "\"\\u00a7c\"",
         "{\"text\":\"a\"}{\"text\":\"b\"}", "{\"color\":\"red\",\"text\":\"x\",\"extra\":[{\"color\":\"blue\",\"text\":\"y\"}]}",
         "\\", "\\\\", "a\\b", "line1\nline2", "tab\tsep", "\u0000", "\u001f", "\ud83d\ude00", "caf\u00e9", "\u4e2d\u6587", "\u2028", "<script>", "'", "=", "&"}) in.add(s);

      // JSON escapes of lone UTF-16 surrogates: the results keep them (the Rust side maps them to U+FFFD)
      for (String s : new String[] {"\"\\ud800\"", "{\"text\":\"\\ud83d\"}", "\"\\ude00\\ud83d\"", "\"\\ud83d\\u0041\"", "{\"\\ud800\":1}"}) in.add(s);

      for (String s : in) all(s);

      // LazilyParsedNumber.longValue / intValue of number texts, including BigDecimal exponent and scale limits
      for (String s : new String[] {"1", "-1", "+5", "2147483648", "-2147483649", "9223372036854775807", "9223372036854775808", "-9223372036854775809",
         "12345678901234567890", "18446744073709551616", "99999999999999999999.9e0", "1.5", "-1.5", "0.99", "-0.99", "1e1", "1E+3", "-1e3", "1e19", "123e5",
         "1e-5", "1e9999", "1e-9999", "1e10000", "1e-10000", "1e00000000000000000000000000005", "1e00000000000", "1e+00000000000000000000009",
         "1e2147483647", "1e-2147483647", "1e2147483648", "1e-2147483648", "1e2147483649", "1e-2147483649", "1.5e2147483647", "1.5e-2147483646",
         "12.34e-2147483645", "12.34e-2147483646", "0.000e-2147483647", "0e2147483648", "0e-2147483648", "1e9999999999", "1e-9999999999",
         "1e12345678901", "1e9223372036854775807", "1e-9223372036854775807", "1e-9223372036854775808", "1e9223372036854775808", "0e-9223372036854775808",
         "0e99999999999", "-0e5", "1e", "1e+", "1e-", "", "-", "+", ".", "abc", "1.e3", "1.2.3", "1x", "x1e3", "1e 3", "1e5x", "123456789012345678x", "1234567890123456789x", "-123456789012345678x", "1".repeat(10001),
         "1".repeat(10000), "1".repeat(9990) + "e5"}) {
         for (String op : new String[] {"lazy_long", "lazy_int"}) {
            String res;
            try {
               LazilyParsedNumber n = new LazilyParsedNumber(s);
               res = op.equals("lazy_long") ? Long.toString(n.longValue()) : Integer.toString(n.intValue());
            } catch (Throwable t) {
               res = err(t);
            }
            line(op, s, res);
         }
      }

      // constants
      line("empty_component", "", LegacyComponentDataFixUtils.createEmptyComponent(NbtOps.INSTANCE).asString("?"));
      line("plain_component", "a\"b", LegacyComponentDataFixUtils.createPlainTextComponent(NbtOps.INSTANCE, "a\"b").asString("?"));
      line("translatable_component", "k.x", LegacyComponentDataFixUtils.createTranslatableComponent(NbtOps.INSTANCE, "k.x").asString("?"));
      Path out = Path.of(args[0]);
      Files.writeString(out, OUT.toString());
      System.err.println(in.size() + " inputs");
   }
}
