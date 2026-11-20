import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for mc_core::java_string::to_lower_case_root: `String.toLowerCase(Locale.ROOT)` of every code point
 * that changes ("cp" lines: code point, result units), and of random strings built from letters, final-sigma contexts,
 * word-break characters (punctuation, digits, marks, format and line separator characters, CJK, surrogates) ("str" lines:
 * input units, result units; units are lowercase hex joined by commas).
 * Usage: crates/mc-core/tests/fixtures/java_case/gen.sh
 * Output: crates/mc-core/tests/fixtures/java_case/golden.txt.gz
 */
public class CaseGolden {
   static String hex(String s) {
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < s.length(); i++) sb.append(i > 0 ? "," : "").append(Integer.toHexString(s.charAt(i)));
      return sb.toString();
   }

   static final String[] ALPHABET = {
      "\u03a3", "\u03a3", "\u03a3", "\u03c3", "\u03c2", "\u0391", "\u03b1", "\u0392", "a", "B", "i", "I", "\u0130", "\u01c5",
      " ", " ", ".", "'", "\"", "-", ",", ":", "_", "\u00b7", "\u2027", "\u00ad", "\u200b", "\u200d", "1", "9", "\u0663", "\u00b2",
      "\u0301", "\u0345", "\u20dd", "\u0903", "\n", "\t", "\r", "\u2028", "\u2029", "\u00a0", "\u3000", "\u00aa", "\u24b6", "\u24d0",
      "\u02b0", "\u2160", "\u1d2c", "\u4e00", "\u3042", "\u30a2", "\u30fc", "\u3099", "\u0964", "$", "#", "%", "&", "\u00a2",
      "\u066b", "\u0e01", "\ud835\udc00", "\ud835\udc1a", "\ud801\udc00", "\ud800", "\udc00", "\ufeff", "\u0061\u0301"
   };

   public static void main(String[] args) throws Exception {
      StringBuilder out = new StringBuilder();
      for (int cp = 0; cp <= 0x10ffff; cp++) {
         if (cp >= 0xd800 && cp <= 0xdfff) continue;
         String s = new String(Character.toChars(cp));
         String l = s.toLowerCase(Locale.ROOT);
         if (!l.equals(s)) out.append("cp\t").append(Integer.toHexString(cp)).append('\t').append(hex(l)).append('\n');
      }
      String[] fixed = {
         "\u03a3", "A\u03a3", "\u0391\u03a3", "\u039f\u0394\u039f\u03a3", "\u039f\u0394\u039f\u03a3 \u0391", "\u039f\u0394\u03a3\u0391",
         "\u03a3\u0391", "A.\u03a3", "A-\u03a3", "A'\u03a3", "A \u03a3", "A\u03a3.", "A\u03a3.B", "A\u03a31", "A1\u03a3", "1\u03a3",
         "A\u0301\u03a3", "A\u03a3\u0301", "\u03a3\u03a3", "A\u03a3\u03a3", "A\u03a3\u03a3 ", "A\u03a3\u03a3A", "\u0391\u03a3\u0391\u03a3",
         "\u0130", "I\u0130", "A\u0130\u03a3", "\ud800\u03a3", "A\ud800\u03a3", "\u03a3\ud800"
      };
      for (String s : fixed) out.append("str\t").append(hex(s)).append('\t').append(hex(s.toLowerCase(Locale.ROOT))).append('\n');
      Random r = new Random(20261120);
      for (int i = 0; i < 40000; i++) {
         int len = 1 + r.nextInt(i % 4 == 0 ? 14 : 7);
         StringBuilder sb = new StringBuilder();
         for (int j = 0; j < len; j++) sb.append(ALPHABET[r.nextInt(ALPHABET.length)]);
         String s = sb.toString();
         out.append("str\t").append(hex(s)).append('\t').append(hex(s.toLowerCase(Locale.ROOT))).append('\n');
      }
      Files.writeString(Path.of(args[0]), out.toString());
   }
}
