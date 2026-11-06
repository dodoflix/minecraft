// Generates ident.txt.
// Usage: scripts/vanilla-java.sh -o crates/mc-core/tests/fixtures/ident_uuid/ident.txt crates/mc-core/tests/fixtures/ident_uuid/IdentGolden.java
// Output: crates/mc-core/tests/fixtures/ident_uuid/ident.txt
// Strings are escaped: every UTF-16 unit outside 0x20..0x7e and backslash becomes backslash-u plus 4 lowercase hex digits.
import java.util.*;
import net.minecraft.IdentifierException;
import net.minecraft.resources.Identifier;

public class IdentGolden {
   static String esc(String s) {
      if (s == null) return "null";
      StringBuilder b = new StringBuilder();
      for (char c : s.toCharArray()) {
         if (c < 0x20 || c > 0x7e || c == '\\') b.append(String.format("\\u%04x", (int) c));
         else b.append(c);
      }
      return b.toString();
   }

   static String parse(String s) {
      try { return "ok:" + esc(Identifier.parse(s).toString()); } catch (IdentifierException e) { return "err:" + esc(e.getMessage()); }
   }

   public static void main(String[] args) {
      StringBuilder out = new StringBuilder();
      String[] inputs = {
         "", ":", "stone", "minecraft:stone", "minecraft:", ":stone", "a:b:c", "a:b/c/d", "a/b:c", "Minecraft:stone", "minecraft:Stone",
         "my_mod:blocks/ore.iron-1", "..:x", ".:x", "a.b:c.d", "a-b:c-d", "a_b:c_d", "-:-", "_:_", "9:9", "ns:", "ns:/", "ns:a//b", "ns:../x",
         "a b:c", "a:b c", "a:b\tc", "a:b\nc", "é:x", "x:é", "\u00e9", "ns:\u4e2d\u6587", "\ud83d\ude00:x", "x:\ud83d\ude00", "a::b", "::", "a:b:", "namespace:path/with/many/segments/and_underscores-and-dashes.123",
         "UPPER", "mixed:Case", "\\", "a\\b", "a:b\\c", "realms:thing", " ", ":" + "x".repeat(40), "x".repeat(300) + ":" + "y".repeat(300), "\u0000", "a:\u0000"
      };
      List<Identifier> valid = new ArrayList<>();
      for (String in : inputs) {
         Identifier t = Identifier.tryParse(in);
         String p = parse(in);
         out.append("id\t").append(esc(in)).append('\t').append(t == null ? "null" : esc(t.toString())).append('\t').append(p)
            .append('\t').append(Identifier.isValidPath(in)).append('\t').append(Identifier.isValidNamespace(in));
         if (t != null) {
            valid.add(t);
            out.append('\t').append(t.hashCode()).append('\t').append(esc(t.toShortString())).append('\t').append(esc(t.toLanguageKey()))
               .append('\t').append(esc(t.toDebugFileName())).append('\t').append(esc(t.toShortLanguageKey())).append('\t').append(esc(t.toLanguageKey("pre", "suf")));
         }
         out.append('\n');
      }
      // namespace x path matrix
      String[] nss = {"minecraft", "", "mod_a", "Mod", "..", ".", "a b", "a/b", "é", "a:b"};
      String[] paths = {"stone", "", "a/b", "A", "a:b", "a b", "../x", ".", "é"};
      for (String ns : nss) {
         for (String p : paths) {
            String r1, r2;
            try { r1 = "ok:" + esc(Identifier.fromNamespaceAndPath(ns, p).toString()); } catch (IdentifierException e) { r1 = "err:" + esc(e.getMessage()); }
            Identifier tb = Identifier.tryBuild(ns, p);
            r2 = tb == null ? "null" : esc(tb.toString());
            out.append("pair\t").append(esc(ns)).append('\t').append(esc(p)).append('\t').append(r1).append('\t').append(r2).append('\n');
         }
      }
      // bySeparator
      for (String in : new String[]{"a/b", "/b", "a/", "a:b/c", "a#b", "#b", "ab", "A/b", "a/B", "é/x", ""}) {
         for (char sep : new char[]{'/', '#'}) {
            String r1, r2;
            try { r1 = "ok:" + esc(Identifier.bySeparator(in, sep).toString()); } catch (IdentifierException e) { r1 = "err:" + esc(e.getMessage()); }
            Identifier t = Identifier.tryBySeparator(in, sep);
            r2 = t == null ? "null" : esc(t.toString());
            out.append("sep\t").append(esc(in)).append('\t').append(sep).append('\t').append(r1).append('\t').append(r2).append('\n');
         }
      }
      // withPath / prefix / suffix
      Identifier base = Identifier.parse("mod:base/x");
      for (String s : new String[]{"", "p/", "Q", "a:b", "é", "-"}) {
         String r1, r2, r3;
         try { r1 = esc(base.withPath(s).toString()); } catch (IdentifierException e) { r1 = "err:" + esc(e.getMessage()); }
         try { r2 = esc(base.withPrefix(s).toString()); } catch (IdentifierException e) { r2 = "err:" + esc(e.getMessage()); }
         try { r3 = esc(base.withSuffix(s).toString()); } catch (IdentifierException e) { r3 = "err:" + esc(e.getMessage()); }
         out.append("with\t").append(esc(s)).append('\t').append(r1).append('\t').append(r2).append('\t').append(r3).append('\n');
      }
      // DataResult error text of Identifier.read(String)
      for (String in : new String[]{"ok:fine", "Bad:Id", "a:b c"}) {
         var dr = Identifier.read(in);
         out.append("read\t").append(esc(in)).append('\t').append(esc(dr.isSuccess() ? dr.getOrThrow().toString() : "err:" + dr.error().get().message())).append('\n');
      }
      // ordering (path first, then namespace) + equality
      List<Identifier> sorted = new ArrayList<>(valid);
      Collections.sort(sorted);
      StringJoiner j = new StringJoiner(" ");
      for (Identifier i : sorted) j.add(esc(i.toString()));
      out.append("sorted\t").append(j).append('\n');
      Identifier[] cmp = {Identifier.parse("a:b"), Identifier.parse("b:a"), Identifier.parse("a:a"), Identifier.parse("b:b"), Identifier.parse("a:b/c"), Identifier.parse("a:b-c"), Identifier.parse("a:b.c")};
      for (Identifier x : cmp) for (Identifier y : cmp) out.append("cmp\t").append(x).append('\t').append(y).append('\t').append(Integer.signum(x.compareTo(y))).append('\t').append(x.equals(y)).append('\n');
      System.out.print(out);
   }
}
