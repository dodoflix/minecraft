import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.FileOutputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.network.Utf8String;
import net.minecraft.network.protocol.status.ClientboundStatusResponsePacket;

/**
 * Golden generator for crates/mc-protocol/tests/golden/status_json.rs: vanilla's status JSON path over crafted lenient/malformed
 * inputs.
 * Usage: scripts/vanilla-java.sh -o crates/mc-protocol/tests/fixtures/status_json.txt.gz crates/mc-protocol/tests/fixtures/StatusJsonGolden.java crates/mc-protocol/tests/fixtures/status_json.txt.gz
 * Output: crates/mc-protocol/tests/fixtures/status_json.txt.gz (the program writes plain text; the wrapper gzips it)
 *
 * Line = hex(input) TAB parse TAB status, where
 *   parse  = "ok:" hex(JsonParser.parseString(input).toString()) | "err:" hex(exception message)
 *   status = "ok:" hex(text of ClientboundStatusResponsePacket.STREAM_CODEC.encode(decode(input))) | "err:" hex(exception class + ": " + message)
 * (`LenientJsonParser.parse` is `JsonParser.parseString`.)
 */
public class StatusJsonGolden {
   static String hex(String s) {
      StringBuilder b = new StringBuilder();
      for (byte x : s.getBytes(StandardCharsets.UTF_8)) {
         b.append(String.format(java.util.Locale.ROOT, "%02x", x));
      }
      return b.toString();
   }

   static String parse(String input) {
      try {
         return "ok:" + hex(JsonParser.parseString(input).toString());
      } catch (Throwable t) {
         return "err:" + hex(String.valueOf(t.getMessage()));
      }
   }

   static String status(String input) {
      try {
         ByteBuf in = Unpooled.buffer();
         Utf8String.write(in, input, 32767);
         ClientboundStatusResponsePacket p = ClientboundStatusResponsePacket.STREAM_CODEC.decode(in);
         ByteBuf out = Unpooled.buffer();
         ClientboundStatusResponsePacket.STREAM_CODEC.encode(out, p);
         return "ok:" + hex(Utf8String.read(out, 32767));
      } catch (Throwable t) {
         return "err:" + hex(t.getClass().getName() + ": " + t.getMessage());
      }
   }

   static final String SEED1 = "{\"description\":\"A Minecraft Server\",\"players\":{\"max\":20,\"online\":3,\"sample\":[{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\",\"name\":\"Notch\"}]},\"version\":{\"name\":\"26.3\",\"protocol\":777},\"enforcesSecureChat\":true}";
   static final String SEED2 = "{\"description\":{\"text\":\"hi\",\"color\":\"red\",\"extra\":[{\"text\":\"x\"}]},\"favicon\":\"data:image/png;base64,iVBORw0KGgo=\"}";
   static final String SEED3 = "[1,{\"a\":[true,null,\"s\"]},2.5e3,-0]";

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      Set<String> cases = new LinkedHashSet<>();
      // hand-written lenient / malformed documents
      String[] hand = {
         "", " ", "\n\t\r ", "\ufeff", "\ufeff{}", "{}", "[]", "null", "NULL", "Null", "nUll", "nul", "nullx", "null x", "null garbage", "true", "false", "TRUE", "True", "tRuE", "1", "-0", "0", "01", "-", "+1", ".5", "1.", "1e", "1e+", "NaN", "Infinity", "-Infinity", "0x10", "abc", "'abc'", "\"abc\"", "'a\\'b'", "\"a\\\"b\"",
         "{} ", "{}\n", "{} x", "{} {}", "{}{}", "{} // c", "{} /* c */", "{} # c", "{} /", "{} /x", "// c\n{}", "/* c */{}", "# c\n{}", "/* unterminated", "{/*c*/}", "{ //c\n}", "{#c\n}",
         ")]}'\n{}", ")]}'\n", ")]}'", ")]}' {}", "\ufeff)]}'\n{}",
         "{'a':'b'}", "{a:b}", "{a:1}", "{a=1}", "{a=>1}", "{a:1;b:2}", "{a:1,b:2}", "{\"a\":1,}", "{,}", "{;}", "{\"a\"}", "{\"a\":}", "{:1}", "{\"a\" 1}", "{\"a\"=1}", "{\"a\"=>1}", "{\"a\":1;}", "{a b:1}", "{1:2}", "{null:1}", "{true:1}", "{\"\":1}", "{'':1}", "{\"a\":1 \"b\":2}", "{\"a\":1;\"b\":2}",
         "[1,2", "[1,2,]", "[,]", "[,1]", "[1,,2]", "[;]", "[1;2]", "[1 2]", "[1;;2]", "[ ]", "[,,]", "['a','b']", "[a,b]", "[a b]", "[true,false,TRUE]", "[1,2]x", "[1,2] x", "[[]]", "]", "}", "[}", "{]", ",", ";", ":",
         "{\"a\":1,\"a\":2}", "{\"a\":{\"b\":1},\"a\":[2]}", "{\"b\":1,\"a\":2,\"b\":3}", "{\"a\":null,\"b\":1}", "[null]",
         "\"\\u00e9\\u2028\\u2029\\u007f\\u0001\"", "\"\\ud83d\\ude00\"", "\"\ud83d\ude00\"", "\"\\u12\"", "\"\\u12g4\"", "\"\\x\"", "\"\\'\"", "\"\\/\"", "\"\\\n\"", "\"\\", "\"\\u", "\"abc", "'abc", "\"a\nb\"", "\"a\tb\"", "\"a\u0001b\"", "\"\\b\\f\\n\\r\\t\"", "\"\\B\"",
         "abc def", "a/b", "a#b", "a;b", "a=b", "a\\b", "a\\u0041b", "[a/b]", "[a#b]", "[a;b]", "[a=b]", "{a\\b:1}", "{a#b:1}",
         "1/2", "1#2", "1;2", "1=2", "1\\2", "[1/2]", "[1/]", "[1/", "[1 /* c */ , 2]", "1 // c", "1 /* c */", "12abc", "1.5.5", "1e5e5", "--1", "-+1", "1e+5", "1E-5", "1e5", "0e0", "0.0", "-0.0", "0.5", "00", "0123", "9223372036854775807", "9223372036854775808", "-9223372036854775808", "-9223372036854775809", "123456789012345678901234567890",
         "\f{}", "{}\f", "{\f}", "\u000b{}", "\u0000", "{\u0000:1}", "{a:\u0000}",
         "{\"description\":", "{\"description\"", "{\"description", "{", "[", "{\"a\":[", "{\"a\":{",
         "{\"description\":\"x\"}trailing", "{\"description\":\"x\"} trailing", "{\"description\":\"x\"}\n\n", "{\"description\":\"x\"}//c", "{\"description\":\"x\"}/*c*/",
      };
      for (String h : hand) {
         cases.add(h);
      }
      // status-shaped documents
      String[] statusHand = {
         "{\"description\":\"hello\"}", "{description:hello}", "{description:'hello'}", "{'description':'hello'}", "{\"description\":null}", "{\"description\":123}", "{\"description\":true}", "{\"description\":[]}", "{\"description\":[\"a\",\"b\"]}", "{\"description\":{}}", "{\"description\":{\"text\":1}}", "{\"description\":{\"text\":\"<&='>\\u2028\\u2029\"}}", "{\"description\":\"\\u00a7ccolor <b>&amp;\"}", "{\"description\":{\"translate\":\"chat.type.text\",\"with\":[1,2.5,\"s\",true,1e2]}}", "{\"description\":{\"translate\":\"x\",\"with\":[1e400]}}", "{\"description\":{\"translate\":\"x\",\"with\":[12345678901234567890123]}}", "{\"description\":{\"translate\":\"x\",\"with\":[1.0,0.1,1e-5,100000000000000000000.5]}}", "{\"description\":{\"text\":\"a\",\"bold\":true,\"italic\":\"yes\"}}", "{\"description\":{\"text\":\"a\",\"color\":\"#ff00ff\"}}", "{\"description\":{\"text\":\"a\",\"extra\":[\"b\",{\"text\":\"c\"}]}}", "{\"description\":\"\"}", "{\"description\":{\"text\":\"\"}}", "{\"description\":\"\\ud83d\\ude00\"}",
         "{\"players\":{\"max\":20,\"online\":0}}", "{\"players\":{\"max\":20}}", "{\"players\":{\"online\":0}}", "{\"players\":{}}", "{\"players\":[]}", "{\"players\":null}", "{\"players\":5}", "{\"players\":{\"max\":\"20\",\"online\":0}}", "{\"players\":{\"max\":true,\"online\":0}}", "{\"players\":{\"max\":null,\"online\":0}}", "{\"players\":{\"max\":[1],\"online\":0}}", "{\"players\":{\"max\":{},\"online\":0}}", "{\"players\":{\"max\":1,\"online\":2,\"sample\":null}}", "{\"players\":{\"max\":1,\"online\":2,\"sample\":5}}", "{\"players\":{\"max\":1,\"online\":2,\"sample\":{}}}", "{\"players\":{\"max\":1,\"online\":2,\"sample\":[]}}", "{\"players\":{\"max\":1,\"online\":2,\"sample\":[null]}}", "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\",\"name\":\"a\"},{\"id\":\"bad\",\"name\":\"b\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\",\"name\":\"<&='>\\u2028\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"1-2-3-4-5\",\"name\":\"short\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"0x1-0x2-0x3-0x4-0x5\",\"name\":\"hex\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"ffffffff-ffff-ffff-ffff-ffffffffffff\",\"name\":\"max\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"069A79F4-44E9-4726-A5BE-FCA90E38AAF5\",\"name\":\"upper\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"-1--1--1--1--1\",\"name\":\"neg\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"+1-+2-+3-+4-+5\",\"name\":\"plus\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"069a79f444e947 26-a5be-fca90e38aaf5\",\"name\":\"x\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":5,\"name\":\"x\"}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\",\"name\":5}]}}",
         "{\"players\":{\"max\":1,\"online\":2,\"sample\":[{\"id\":\"069a79f4-44e9-4726-a5be-fca90e38aaf5\"}]}}",
         "{\"version\":{\"name\":\"n\",\"protocol\":1}}", "{\"version\":{\"name\":\"n\"}}", "{\"version\":{\"protocol\":1}}", "{\"version\":{\"name\":5,\"protocol\":1}}", "{\"version\":{\"name\":\"n\",\"protocol\":\"1\"}}", "{\"version\":[]}", "{\"version\":\"x\"}", "{\"version\":{\"name\":\"<&='>\\u2028\\u2029\\u0001\",\"protocol\":-1}}",
         "{\"favicon\":\"data:image/png;base64,\"}", "{\"favicon\":\"data:image/png;base64,QQ==\"}", "{\"favicon\":\"data:image/png;base64,QQ\"}", "{\"favicon\":\"data:image/png;base64,QQ=\"}", "{\"favicon\":\"data:image/png;base64,Q\"}", "{\"favicon\":\"data:image/png;base64,QUI\"}", "{\"favicon\":\"data:image/png;base64,QUI=\"}", "{\"favicon\":\"data:image/png;base64,QUJD\"}", "{\"favicon\":\"data:image/png;base64,QUJD=\"}", "{\"favicon\":\"data:image/png;base64,QUJD====\"}", "{\"favicon\":\"data:image/png;base64,QU=J\"}", "{\"favicon\":\"data:image/png;base64,Q===\"}", "{\"favicon\":\"data:image/png;base64,====\"}", "{\"favicon\":\"data:image/png;base64,=\"}", "{\"favicon\":\"data:image/png;base64,QUJD\\nREVG\"}", "{\"favicon\":\"data:image/png;base64,QU JD\"}", "{\"favicon\":\"data:image/png;base64,QUJ_\"}", "{\"favicon\":\"data:image/png;base64,QUJ-\"}", "{\"favicon\":\"data:image/png;base64,QUJD\\u00e9\"}", "{\"favicon\":\"data:image/png;base64,QQ==QQ==\"}", "{\"favicon\":\"data:image/png;base64,QQ==x\"}", "{\"favicon\":\"data:image/jpeg;base64,QQ==\"}", "{\"favicon\":\"QQ==\"}", "{\"favicon\":5}", "{\"favicon\":null}", "{\"favicon\":\"\"}", "{\"favicon\":\"data:image/png;base64,/+/+\"}", "{\"favicon\":\"data:image/png;base64,_-_-\"}",
         "{\"enforcesSecureChat\":true}", "{\"enforcesSecureChat\":false}", "{\"enforcesSecureChat\":\"true\"}", "{\"enforcesSecureChat\":1}", "{\"enforcesSecureChat\":null}", "{enforcesSecureChat:TRUE}", "{enforcesSecureChat:True}", "{\"enforcesSecureChat\":[true]}",
         "{\"unknown\":1}", "{\"players\":{\"max\":1,\"online\":2},\"players\":{\"max\":3,\"online\":4}}", "{\"players\":{\"max\":1,\"max\":7,\"online\":2}}",
      };
      for (String h : statusHand) {
         cases.add(h);
      }
      // number forms through Codec.INT
      String[] nums = {
         "0", "1", "-1", "20", "1.0", "1.9", "-1.9", "0.5", "-0.5", "1e2", "1E2", "1e+2", "1E+2", "1e-2", "1.5e1", "1.5e0", "12e-1", "123e-2", "2147483647", "2147483648", "-2147483648", "-2147483649", "4294967295", "4294967296", "4294967297", "9223372036854775807", "9223372036854775808", "18446744073709551615", "18446744073709551616", "18446744073709551617", "-9223372036854775809", "1e10", "1e9", "1e18", "1e19", "1e31", "1e32", "1e33", "1e63", "1e64", "1e65", "1e100", "1e400", "-1e400", "1e-400", "123456789012345678901234567890", "-123456789012345678901234567890", "1234567890123456789012345678901234567890.5", "12345678901234567890.12345678", "1.5e30", "7e-1", "9.99e2", "0e0", "0.0", "-0", "-0.0", "-0e5", "1e00005", "1e+0", "1e-0", "0.00001e5", "1e9999", "1e-9999", "1e10000", "1e-10000", "1.0e9998", "0.1e10002",
         "01", "+1", ".5", "1.", "1e", "NaN", "Infinity", "-Infinity", "0x10", "'5'", "\"5\"", "five", "TRUE", "true", "null", "[1]", "{}", "1x", "1_0", "--1", "1e5e5",
      };
      for (String n : nums) {
         cases.add("{\"players\":{\"max\":" + n + ",\"online\":1}}");
         cases.add("{\"version\":{\"name\":\"v\",\"protocol\":" + n + "}}");
         cases.add("[" + n + "]");
         cases.add(n);
         cases.add("{players:{max:" + n + ";online:" + n + "}}");
      }
      StringBuilder big = new StringBuilder("{\"players\":{\"max\":1");
      for (int i = 0; i < 1100; i++) {
         big.append('0');
      }
      cases.add(big + ",\"online\":1}}");
      StringBuilder big2 = new StringBuilder("{\"players\":{\"max\":1");
      for (int i = 0; i < 1023; i++) {
         big2.append('0');
      }
      cases.add(big2 + ",\"online\":1}}");
      StringBuilder big3 = new StringBuilder("{\"players\":{\"max\":1");
      for (int i = 0; i < 1022; i++) {
         big3.append('0');
      }
      cases.add(big3 + ",\"online\":1}}");
      StringBuilder big4 = new StringBuilder("{\"players\":{\"max\":1");
      for (int i = 0; i < 10005; i++) {
         big4.append('0');
      }
      cases.add(big4 + ".5,\"online\":1}}");
      StringBuilder big5 = new StringBuilder("{\"players\":{\"max\":1");
      for (int i = 0; i < 1100; i++) {
         big5.append('0');
      }
      cases.add(big5 + "e1,\"online\":1}}");
      StringBuilder long6 = new StringBuilder("{\"players\":{\"max\":7,\"online\":1},\"description\":\"");
      for (int i = 0; i < 2100; i++) {
         long6.append((char) ('a' + i % 26));
      }
      cases.add(long6 + "\"}");
      // nesting depth
      for (int d : new int[] {1, 2, 254, 255, 256, 257, 300}) {
         cases.add("[".repeat(d) + "]".repeat(d));
         cases.add("{\"a\":".repeat(d) + "1" + "}".repeat(d));
         cases.add("{\"description\":" + "[".repeat(d) + "]".repeat(d) + "}");
      }
      cases.add("[".repeat(256));
      // mutations of the seeds: every single-character deletion, quote swap, and junk insertion at every position
      String[] junk = {"/*c*/", "//c\n", "#c\n", " ", "\n", ";", "=", "=>", "\f", "\u0000", "\ufeff", "'", "\"", ",", ":", "\\", "/"};
      for (String seed : new String[] {SEED1, SEED2, SEED3}) {
         cases.add(seed);
         cases.add(seed.replace('"', '\''));
         cases.add(seed.replaceAll("\"([A-Za-z]+)\":", "$1:"));
         cases.add(seed.replaceAll("\"([A-Za-z0-9_.\\- ]+)\"", "$1"));
         cases.add(seed.replace(",", ";"));
         cases.add(seed.replace(":", "="));
         cases.add(seed.replace(":", "=>"));
         cases.add(seed.replace("true", "TRUE").replace("null", "NULL"));
         for (int i = 0; i <= seed.length(); i++) {
            cases.add(seed.substring(0, i));
            if (i < seed.length()) {
               cases.add(seed.substring(0, i) + seed.substring(i + 1));
               cases.add(seed.substring(0, i) + "'" + seed.substring(i + 1));
            }
            for (String j : junk) {
               cases.add(seed.substring(0, i) + j + seed.substring(i));
            }
         }
      }
      OutputStream os = new FileOutputStream(args[0]);
      for (String c : cases) {
         if (c.length() > 32767) {
            throw new IllegalStateException("case too long");
         }
         os.write((hex(c) + "\t" + parse(c) + "\t" + status(c) + "\n").getBytes(StandardCharsets.UTF_8));
      }
      os.close();
      System.out.println(cases.size() + " cases");
   }
}
