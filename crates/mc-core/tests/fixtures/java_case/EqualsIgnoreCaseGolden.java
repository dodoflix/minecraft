import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for mc_core::java_case::equals_ignore_case: `Character.toUpperCase(int)` / `Character.toLowerCase(int)`
 * of every code point that changes ("up"/"lo" lines: code point, result), `String.equalsIgnoreCase` of every pair of
 * code points that share a case class ("eq" lines: both code points, 1 or 0), and of random string pairs built from
 * the members of a few case classes, Latin-1 and supplementary characters so that all four coder combinations and the
 * surrogate pair path run ("str" lines: units of both strings as lowercase hex joined by commas, 1 or 0).
 * Usage: crates/mc-core/tests/fixtures/java_case/gen.sh
 * Output: crates/mc-core/tests/fixtures/java_case/equals_ignore_case.txt.gz
 */
public class EqualsIgnoreCaseGolden {
   static String hex(String s) {
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < s.length(); i++) sb.append(i > 0 ? "," : "").append(Integer.toHexString(s.charAt(i)));
      return sb.toString();
   }

   static String str(int cp) {
      return new String(Character.toChars(cp));
   }

   public static void main(String[] args) throws Exception {
      StringBuilder out = new StringBuilder();
      Map<Integer, List<Integer>> classes = new TreeMap<>();
      for (int cp = 0; cp <= 0x10ffff; cp++) {
         if (cp >= 0xd800 && cp <= 0xdfff) continue;
         int up = Character.toUpperCase(cp), lo = Character.toLowerCase(cp);
         if (up != cp) out.append("up\t").append(Integer.toHexString(cp)).append('\t').append(Integer.toHexString(up)).append('\n');
         if (lo != cp) out.append("lo\t").append(Integer.toHexString(cp)).append('\t').append(Integer.toHexString(lo)).append('\n');
         classes.computeIfAbsent(Character.toLowerCase(up), k -> new ArrayList<>()).add(cp);
      }
      List<List<Integer>> multi = new ArrayList<>();
      for (List<Integer> members : classes.values()) {
         if (members.size() < 2) continue;
         multi.add(members);
         for (int i = 0; i < members.size(); i++) {
            for (int j = i + 1; j < members.size(); j++) {
               int a = members.get(i), b = members.get(j);
               out.append("eq\t").append(Integer.toHexString(a)).append('\t').append(Integer.toHexString(b)).append('\t')
                  .append(str(a).equalsIgnoreCase(str(b)) ? 1 : 0).append('\n');
            }
         }
      }
      // Non-members next to the members: random code points against the one after them, and against a random one.
      Random r = new Random(20261121);
      for (int i = 0; i < 25000; i++) {
         int cp = r.nextInt(0x30000);
         if (cp >= 0xd7ff && cp <= 0xdfff) continue;
         int other = r.nextInt(3) == 0 ? r.nextInt(0x30000) : cp + 1;
         if (other >= 0xd800 && other <= 0xdfff) continue;
         out.append("eq\t").append(Integer.toHexString(cp)).append('\t').append(Integer.toHexString(other)).append('\t')
            .append(str(cp).equalsIgnoreCase(str(other)) ? 1 : 0).append('\n');
      }
      int[] extra = {
         'a', 'A', 'z', 'Z', 'k', 'K', 0x212a, 0xdf, 0x1e9e, 0xb5, 0x3bc, 0x39c, 0xff, 0x178, 0xe0, 0xc0, 0xd7, 0xf7, 0x130, 0x131,
         0x49, 0x69, 0x3c3, 0x3c2, 0x3a3, 0x1c4, 0x1c5, 0x1c6, 0x1fb3, 0x1fbc, 0x1f80, 0x1f88, 0x10400, 0x10428, 0x1d7ce, '0', ' ', '-',
         0x10a0, 0x2d00, 0x1c90, 0x10d0, 0x4e00, 0x1e9e, 0x2126, 0x3c9, 0x3a9, 0x212b, 0xe5, 0xc5
      };
      String[] pool = new String[extra.length + 12];
      for (int i = 0; i < extra.length; i++) pool[i] = str(extra[i]);
      String[] more = {"\u0100", "\u0101", "ab", "AB", "\ud801\udc00", "\ud801\udc28", "\ud801", "\udc28", "\u00e9", "\u00c9", "\u0301", "\u0645"};
      System.arraycopy(more, 0, pool, extra.length, more.length);
      for (int i = 0; i < 20000; i++) {
         int len = 1 + r.nextInt(i % 5 == 0 ? 8 : 4);
         StringBuilder x = new StringBuilder(), y = new StringBuilder();
         for (int j = 0; j < len; j++) {
            String p = pool[r.nextInt(pool.length)];
            x.append(p);
            int mode = r.nextInt(10);
            if (mode < 6 && p.codePointCount(0, p.length()) == 1) {
               int cp = p.codePointAt(0), up = Character.toUpperCase(cp);
               List<Integer> cls = classes.get(Character.toLowerCase(up));
               y.append(str(cls == null ? cp : cls.get(r.nextInt(cls.size()))));
            } else if (mode < 9) {
               y.append(p);
            } else {
               y.append(pool[r.nextInt(pool.length)]);
            }
         }
         String a = x.toString(), b = y.toString();
         // Lone surrogates cannot be a Rust `str`.
         if (!valid(a) || !valid(b)) continue;
         out.append("str\t").append(hex(a)).append('\t').append(hex(b)).append('\t').append(a.equalsIgnoreCase(b) ? 1 : 0).append('\n');
      }
      Files.writeString(Path.of(args[0]), out.toString());
   }

   static boolean valid(String s) {
      for (int i = 0; i < s.length(); i++) {
         char c = s.charAt(i);
         if (Character.isHighSurrogate(c)) {
            if (i + 1 >= s.length() || !Character.isLowSurrogate(s.charAt(i + 1))) return false;
            i++;
         } else if (Character.isLowSurrogate(c)) return false;
      }
      return true;
   }
}
