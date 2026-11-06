import java.io.*;

/**
 * Dumps Java's Unicode name tables (Character.getName) used by SNBT `\N{...}` escapes
 * (Character.codePointOf) as plain TSV on stdout (the `-o` wrapper gzips it): "N<TAB>HEX<TAB>NAME" for named code points and
 * "B<TAB>START<TAB>END<TAB>BLOCK NAME" for every UnicodeBlock range (names with '_' -> ' ').
 * Usage: scripts/vanilla-java.sh -o crates/mc-nbt/src/snbt/unicode_names.tsv.gz crates/mc-nbt/tests/fixtures/UnicodeNamesGen.java
 * Output: crates/mc-nbt/src/snbt/unicode_names.tsv.gz
 */
public class UnicodeNamesGen {
    public static void main(String[] a) throws Exception {
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(System.out, "UTF-8"))) {
            Character.UnicodeBlock cur = null; int start = 0;
            for (int cp = 0; cp <= 0x110000; cp++) {
                Character.UnicodeBlock b = cp == 0x110000 ? null : Character.UnicodeBlock.of(cp);
                if (b != cur) {
                    if (cur != null) out.print("B\t" + Integer.toHexString(start) + "\t" + Integer.toHexString(cp - 1) + "\t" + cur.toString().replace('_', ' ') + "\n");
                    cur = b; start = cp;
                }
                if (cp == 0x110000) break;
                String n = Character.getName(cp);
                if (n == null) continue;
                String fb = b == null ? null : b.toString().replace('_', ' ') + " " + Integer.toHexString(cp).toUpperCase();
                if (!n.equals(fb)) out.print("N\t" + Integer.toHexString(cp) + "\t" + n + "\n");
            }
        }
    }
}
