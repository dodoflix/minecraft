import net.minecraft.nbt.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/**
 * Generates model_golden.tsv:
 *   "N\t<kind>\t<value-bits-hex>\tbyte\tshort\tint\tlong\tfloatBits\tdoubleBits\tbool"  numeric conversions
 *   "Z\t<hex any-tag bytes>\t<sizeInBytes>"                                              Tag.sizeInBytes over the snbt corpus
 *   "C\t<expected snbt>\t<actual snbt>\t<partial 0/1>\t<result>"                          NbtUtils.compareNbt
 * Usage: scripts/vanilla-java.sh -o crates/mc-nbt/tests/fixtures/model_golden.tsv crates/mc-nbt/tests/fixtures/ModelOracle.java crates/mc-nbt/tests/fixtures/model_golden.tsv crates/mc-nbt/tests/fixtures/snbt_golden.tsv.gz
 * Output: crates/mc-nbt/tests/fixtures/model_golden.tsv (reads snbt_golden.tsv: regenerate that first)
 */
public class ModelOracle {
    static byte[] unhex(String h) {
        byte[] b = new byte[h.length() / 2];
        for (int i = 0; i < b.length; i++) b[i] = (byte) Integer.parseInt(h.substring(2 * i, 2 * i + 2), 16);
        return b;
    }

    static String unesc(String s) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c != '\\') { b.append(c); continue; }
            char n = s.charAt(++i);
            if (n == 'n') b.append('\n'); else if (n == 't') b.append('\t'); else if (n == 'r') b.append('\r'); else if (n == '\\') b.append('\\');
            else if (n == 'x') { b.append((char) Integer.parseInt(s.substring(i + 1, i + 3), 16)); i += 2; }
            else throw new IllegalStateException("bad escape");
        }
        return b.toString();
    }

    static String esc(String s) { return s.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t"); }

    static void num(PrintWriter out, String kind, long bits, NumericTag t) {
        out.println("N\t" + kind + "\t" + Long.toHexString(bits) + "\t" + t.byteValue() + "\t" + t.shortValue() + "\t" + t.intValue() + "\t" + t.longValue() + "\t"
            + Integer.toHexString(Float.floatToIntBits(t.floatValue())) + "\t" + Long.toHexString(Double.doubleToLongBits(t.doubleValue())) + "\t" + (t.byteValue() != 0));
    }

    /** Lines of a gzip-compressed UTF-8 file (the generated corpora are committed as .gz). */
    static List<String> gzLines(String path) throws IOException {
        try (BufferedReader r = new BufferedReader(new InputStreamReader(new java.util.zip.GZIPInputStream(new FileInputStream(path)), StandardCharsets.UTF_8))) {
            return r.lines().toList();
        }
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        net.minecraft.SharedConstants.IS_RUNNING_IN_IDE = false;
        Random rnd = new Random(77);
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            long[] longs = {0, 1, -1, 127, 128, 255, 256, -128, -129, 32767, 32768, 65535, 65536, -32768, -32769, Integer.MAX_VALUE, Integer.MIN_VALUE, (long) Integer.MAX_VALUE + 1, Long.MAX_VALUE, Long.MIN_VALUE, 0x123456789abcdefL};
            List<Long> ls = new ArrayList<>();
            for (long l : longs) ls.add(l);
            for (int i = 0; i < 60; i++) ls.add(rnd.nextLong() >> rnd.nextInt(64));
            for (long l : ls) {
                num(out, "byte", l & 0xff, ByteTag.valueOf((byte) l));
                num(out, "short", l & 0xffff, ShortTag.valueOf((short) l));
                num(out, "int", l & 0xffffffffL, IntTag.valueOf((int) l));
                num(out, "long", l, LongTag.valueOf(l));
            }
            double[] ds = {0.0, -0.0, 0.5, -0.5, 1.5, -1.5, 127.9, 128.0, -128.1, 255.5, 256.0, 32768.7, -32769.2, 65535.9, 2147483647.0, 2147483648.0, -2147483648.0, -2147483649.0, 4294967296.5, 1e18, -1e18, 9.3e18, -9.3e18, 1e300, -1e300, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.MIN_VALUE, Double.MAX_VALUE, 3.999999999, -3.0000001};
            List<Double> dl = new ArrayList<>();
            for (double d : ds) dl.add(d);
            for (int i = 0; i < 60; i++) dl.add((rnd.nextDouble() - 0.5) * Math.pow(10, rnd.nextInt(22)));
            for (double d : dl) {
                num(out, "double", Double.doubleToLongBits(d), DoubleTag.valueOf(d));
                float f = (float) d;
                num(out, "float", Float.floatToIntBits(f) & 0xffffffffL, FloatTag.valueOf(f));
            }
            List<String> hexes = new ArrayList<>();
            List<String> snbts = new ArrayList<>();
            for (String line : gzLines(args[1])) {
                String[] f = line.split("\t", -1);
                if (f[1].equals("O")) { hexes.add(f[4]); snbts.add(unesc(f[2])); }
            }
            for (int i = 0; i < hexes.size(); i += 3) {
                Tag t = NbtIo.readAnyTag(new DataInputStream(new ByteArrayInputStream(unhex(hexes.get(i)))), NbtAccounter.unlimitedHeap());
                out.println("Z\t" + hexes.get(i) + "\t" + t.sizeInBytes());
            }
            TagParser<Tag> parser = TagParser.create(NbtOps.INSTANCE);
            List<String[]> pairs = new ArrayList<>();
            String[][] hand = {
                {"{a:1}", "{a:1,b:2}"}, {"{a:1,b:2}", "{a:1}"}, {"{a:1}", "{a:1b}"}, {"{a:{b:1}}", "{a:{b:1,c:2},d:3}"}, {"{a:{b:1}}", "{a:{b:2}}"},
                {"[1,2]", "[3,2,1]"}, {"[1,2]", "[1]"}, {"[]", "[]"}, {"[]", "[1]"}, {"[1]", "[]"}, {"[{a:1}]", "[{a:1,b:2},{c:3}]"}, {"[{a:1},{c:3}]", "[{c:3,x:1},{a:1}]"}, {"[1,1]", "[1]"},
                {"{}", "{a:1}"}, {"{a:[1]}", "{a:[1,2]}"}, {"{a:[]}", "{a:[1]}"}, {"1", "1"}, {"1", "1b"}, {"\"x\"", "\"x\""}, {"[B;1]", "[B;1]"}, {"[I;1]", "[I;1,2]"}, {"1.5", "1.5"}, {"{a:[1,2]}", "{a:[2,1]}"},
            };
            pairs.addAll(Arrays.asList(hand));
            for (int i = 0; i + 1 < snbts.size() && pairs.size() < 400; i += 11) pairs.add(new String[]{snbts.get(i), snbts.get(i + 1)});
            for (int i = 0; i < snbts.size() && pairs.size() < 500; i += 29) pairs.add(new String[]{snbts.get(i), snbts.get(i)});
            for (String[] p : pairs) for (int partial = 0; partial < 2; partial++) {
                Tag e = parser.parseFully(p[0]), a = parser.parseFully(p[1]);
                out.println("C\t" + esc(p[0]) + "\t" + esc(p[1]) + "\t" + partial + "\t" + NbtUtils.compareNbt(e, a, partial == 1));
            }
        }
    }
}
