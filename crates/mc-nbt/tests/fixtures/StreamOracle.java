import net.minecraft.nbt.*;
import net.minecraft.nbt.visitors.*;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/**
 * Generates stream_golden.tsv from the real NbtIo.parse / Tag.acceptAsRoot and the nbt.visitors classes.
 * One line per input: "P\t<hex of a named-root NBT>\t<cell>...". Cells (same order as tests/golden/stream_golden.rs):
 *   parse cells:  "<status>|<accounter usage>|..." where status is "OK" or "<exception>:<message>" (escaped)
 *   accept cells: operate on the tag read back with NbtIo.readUnnamedTag ("-" when that fails).
 * Inputs: a deterministic selection of the "O" rows of snbt_golden.tsv (named hex; non-compound rows get an empty name),
 * truncations of some of them, and hand-made cases (SNBT-built tags, malformed bytes).
 * Usage: scripts/vanilla-java.sh -o crates/mc-nbt/tests/fixtures/stream_golden.tsv crates/mc-nbt/tests/fixtures/StreamOracle.java crates/mc-nbt/tests/fixtures/stream_golden.tsv crates/mc-nbt/tests/fixtures/snbt_golden.tsv.gz
 * Output: crates/mc-nbt/tests/fixtures/stream_golden.tsv (reads snbt_golden.tsv: regenerate that first)
 */
public class StreamOracle {
    static final long QUOTA = 104857600L;

    static String hex(byte[] b) {
        StringBuilder sb = new StringBuilder();
        for (byte x : b) sb.append(String.format(java.util.Locale.ROOT, "%02x", x));
        return sb.toString();
    }

    static byte[] unhex(String h) {
        h = h.replace(" ", "");
        byte[] b = new byte[h.length() / 2];
        for (int i = 0; i < b.length; i++) b[i] = (byte) Integer.parseInt(h.substring(2 * i, 2 * i + 2), 16);
        return b;
    }

    static String esc(String s) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\') b.append("\\\\");
            else if (c == '\n') b.append("\\n");
            else if (c == '\t') b.append("\\t");
            else if (c == '\r') b.append("\\r");
            else if (c < 0x20) b.append(String.format(java.util.Locale.ROOT, "\\x%02X", (int) c));
            else b.append(c);
        }
        return b.toString();
    }

    static DataInputStream in(byte[] b) {
        return new DataInputStream(new ByteArrayInputStream(b));
    }

    static String status(Throwable e) {
        if (e instanceof EOFException) return "EOF";
        return esc(e.getClass().getSimpleName() + ":" + e.getMessage());
    }

    static String tagStr(Tag t) {
        return t == null ? "null" : esc(t.toString());
    }

    static int idOf(TagType<?> t) {
        for (int i = 0; i < 13; i++) if (TagTypes.getType(i) == t) return i;
        String n = t.getName(); // INVALID[n]
        return Integer.parseInt(n.substring(8, n.length() - 1));
    }

    /** Deterministic BREAK/SKIP/HALT policy; hashes every call. Mirrored in tests/golden/stream_golden.rs. */
    static class Trace implements StreamTagVisitor {
        static final int[][] POLICY = {{0, 0, 0}, {0, 0, 7}, {0, 2, 0}, {1, 0, 0}, {1, 2, 7}, {2, 3, 5}};
        final int mode;
        int n = 0;
        int count = 0;
        long h = 0xcbf29ce484222325L;

        Trace(int mode) { this.mode = mode; }

        void log(String s) {
            count++;
            for (int i = 0; i < s.length(); i++) { h ^= s.charAt(i); h *= 0x100000001b3L; }
            h ^= '\n';
            h *= 0x100000001b3L;
        }

        int roll() {
            long x = (((long) n++) + 1) * 2654435761L + mode * 97L;
            return (int) ((x >>> 8) & 0x7fffffff) % 23;
        }

        ValueResult value() {
            int r = roll();
            int[] p = POLICY[mode];
            if (r < p[0]) return ValueResult.HALT;
            if (r < p[0] + p[1]) return ValueResult.BREAK;
            return ValueResult.CONTINUE;
        }

        EntryResult entry() {
            int r = roll();
            int[] p = POLICY[mode];
            if (r < p[0]) return EntryResult.HALT;
            if (r < p[0] + p[1]) return EntryResult.BREAK;
            if (r < p[0] + p[1] + p[2]) return EntryResult.SKIP;
            return EntryResult.ENTER;
        }

        String cell() { return count + "|" + Long.toHexString(h); }

        public ValueResult visitEnd() { log("E"); return value(); }
        public ValueResult visit(String v) { log("S" + v); return value(); }
        public ValueResult visit(byte v) { log("B" + v); return value(); }
        public ValueResult visit(short v) { log("H" + v); return value(); }
        public ValueResult visit(int v) { log("I" + v); return value(); }
        public ValueResult visit(long v) { log("J" + v); return value(); }
        public ValueResult visit(float v) { log("F" + Float.floatToIntBits(v)); return value(); }
        public ValueResult visit(double v) { log("D" + Double.doubleToLongBits(v)); return value(); }
        public ValueResult visit(byte[] v) {
            StringBuilder s = new StringBuilder("A" + v.length);
            for (byte x : v) s.append(',').append(x);
            log(s.toString());
            return value();
        }
        public ValueResult visit(int[] v) {
            StringBuilder s = new StringBuilder("N" + v.length);
            for (int x : v) s.append(',').append(x);
            log(s.toString());
            return value();
        }
        public ValueResult visit(long[] v) {
            StringBuilder s = new StringBuilder("O" + v.length);
            for (long x : v) s.append(',').append(x);
            log(s.toString());
            return value();
        }
        public ValueResult visitList(TagType<?> t, int size) { log("L" + (byte) idOf(t) + "," + size); return value(); }
        public EntryResult visitEntry(TagType<?> t) { log("e" + (byte) idOf(t)); return entry(); }
        public EntryResult visitEntry(TagType<?> t, String id) { log("k" + (byte) idOf(t) + ":" + id); return entry(); }
        public EntryResult visitElement(TagType<?> t, int index) { log("m" + (byte) idOf(t) + "," + index); return entry(); }
        public ValueResult visitContainerEnd() { log("c"); return value(); }
        public ValueResult visitRootEntry(TagType<?> t) { log("r" + (byte) idOf(t)); return value(); }
    }

    static FieldSelector sel(int type, String name, String... path) {
        return new FieldSelector(List.of(path), TagTypes.getType(type), name);
    }

    static final FieldSelector[][] SETS = {
        {},
        {sel(3, "a")},
        {sel(8, "a"), sel(3, "b")},
        {sel(10, "a")},
        {sel(3, "DataVersion")},
        {sel(3, "b", "a")},
        {sel(3, "DataVersion", "Data"), sel(8, "name", "Data", "version"), sel(3, "DataVersion")},
        {sel(9, "k1"), sel(10, "k0"), sel(3, "k2"), sel(8, "k3")},
        {sel(10, "k29", "k28"), sel(3, "k2", "k0", "k1")},
        {sel(1, "a"), sel(2, "a"), sel(4, "a"), sel(6, "a"), sel(5, "a"), sel(7, "a"), sel(11, "a"), sel(12, "a"), sel(9, "a"), sel(8, "a")},
        {sel(8, "facing", "Properties"), sel(8, "Name")},
        {sel(3, "a"), sel(3, "a")},
        {sel(3, "x", "a", "b", "c")},
    };

    static String run(byte[] b, StreamTagVisitor v, NbtAccounter a) {
        String s;
        try {
            NbtIo.parse(in(b), v, a);
            s = "OK";
        } catch (Throwable e) {
            s = status(e);
        }
        return s + "|" + a.getUsage();
    }

    static List<String> cells(byte[] b) {
        List<String> o = new ArrayList<>();
        CollectToTag c = new CollectToTag();
        NbtAccounter a = new NbtAccounter(QUOTA, 512);
        String r = run(b, c, a);
        o.add(r + "|" + tagStr(c.getResult()));
        long usage = a.getUsage();
        o.add(run(b, new CollectToTag(), new NbtAccounter(usage / 2, 512)));
        o.add(run(b, new CollectToTag(), new NbtAccounter(QUOTA, 3)));
        o.add(run(b, SkipAll.INSTANCE, new NbtAccounter(QUOTA, 512)));
        for (FieldSelector[] set : SETS) {
            CollectFields cf = new CollectFields(set);
            String rc = run(b, cf, new NbtAccounter(QUOTA, 512));
            o.add(rc + "|" + cf.getMissingFieldCount() + "|" + tagStr(cf.getResult()));
            SkipFields sf = new SkipFields(set);
            String rs = run(b, sf, new NbtAccounter(QUOTA, 512));
            o.add(rs + "|" + tagStr(sf.getResult()));
        }
        for (int mode = 0; mode < Trace.POLICY.length; mode++) {
            Trace t = new Trace(mode);
            o.add(run(b, t, new NbtAccounter(QUOTA, 512)) + "|" + t.cell());
        }
        // ---- accept
        Tag tag;
        try {
            tag = NbtIo.readUnnamedTag(in(b), new NbtAccounter(QUOTA, 512));
        } catch (Throwable e) {
            tag = null;
        }
        int accepts = 2 + 2 * SETS.length + Trace.POLICY.length;
        if (tag == null) {
            for (int i = 0; i < accepts; i++) o.add("-");
            return o;
        }
        try {
            CollectToTag c2 = new CollectToTag();
            tag.acceptAsRoot(c2);
            o.add(tagStr(c2.getResult()));
            tag.acceptAsRoot(SkipAll.INSTANCE);
            o.add("OK");
            for (FieldSelector[] set : SETS) {
                CollectFields cf = new CollectFields(set);
                tag.acceptAsRoot(cf);
                o.add(cf.getMissingFieldCount() + "|" + tagStr(cf.getResult()));
                SkipFields sf = new SkipFields(set);
                tag.acceptAsRoot(sf);
                o.add(tagStr(sf.getResult()));
            }
            for (int mode = 0; mode < Trace.POLICY.length; mode++) {
                Trace t = new Trace(mode);
                tag.acceptAsRoot(t);
                o.add(t.cell());
            }
        } catch (Throwable e) {
            throw new RuntimeException("accept failed for " + hex(b), e);
        }
        return o;
    }

    static byte[] named(CompoundTag c) throws IOException {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        NbtIo.write(c, new DataOutputStream(o));
        return o.toByteArray();
    }

    static boolean rich(Tag t) {
        if (t instanceof CompoundTag c) {
            for (String k : c.keySet()) {
                Tag v = c.get(k);
                if (v instanceof CompoundTag || v instanceof ListTag) return true;
            }
        } else if (t instanceof ListTag l) {
            for (Tag v : l) if (v instanceof CompoundTag) return true;
        }
        return false;
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

        Set<String> inputs = new LinkedHashSet<>();
        int richSeen = 0, plainSeen = 0, richTaken = 0;
        for (String line : gzLines(args[1])) {
            String[] f = line.split("\t", -1);
            if (!f[1].equals("O")) continue;
            String any = f[4];
            String named = f[5].equals("-") ? any.substring(0, 2) + "0000" + any.substring(2) : f[5];
            if (any.equals("00")) named = "00";
            if (named.length() > 360) continue;
            Tag t = NbtIo.readAnyTag(in(unhex(any)), new NbtAccounter(QUOTA, 512));
            if (rich(t)) {
                if (richSeen++ % 6 == 0) inputs.add(named);
            } else if (plainSeen++ % 30 == 0) {
                inputs.add(named);
            }
        }
        List<String> selected = new ArrayList<>(inputs);
        for (int i = 0; i < selected.size(); i += 6) {
            byte[] b = unhex(selected.get(i));
            inputs.add(hex(Arrays.copyOf(b, Math.max(0, b.length - 1))));
            inputs.add(hex(Arrays.copyOf(b, b.length / 2)));
        }

        // ---- hand-made, SNBT built
        List<String> snbt = new ArrayList<>(List.of(
            "{DataVersion:4000,Data:{LevelName:\"w\",version:{Id:4000,name:\"26.3\"},DataVersion:4001,k:[1,2,3]},extra:{DataVersion:\"x\"}}",
            "{a:1,b:\"s\",c:{a:2,b:3},d:[{a:1},{a:2}],e:[1,2,3],f:[B;1,2],g:[I;1,2],h:[L;1L]}",
            "{a:[1,\"x\",{b:2}],d:[[1],[2,3]]}",
            "{k0:{k1:{k2:5,k3:\"s\"},k2:1},k1:[1],k2:7,k3:\"t\",k28:{k29:{a:1}},k29:3}",
            "{a:{a:{a:{a:{a:{a:1}}}}}}",
            "{a:{b:{c:{x:1}}},b:{a:1},c:1b}",
            "{DataVersion:1b,a:1L,b:1.5f,c:2.5d,d:2s}",
            "{Properties:{facing:\"north\",x:1},Name:\"minecraft:stone\"}",
            "{a:[],b:[{}],c:[[],[]],d:{},e:[B;],f:[I;],g:[L;]}",
            "{DataVersion:4000,a:{b:{c:{x:1,y:2},z:3},w:4},b:2,k1:{k2:{k3:1}},c:[{a:1,b:2},{a:3}]}",
            "{c:\"a\\u00e9\\U0001F600\"}"));
        StringBuilder many = new StringBuilder("{");
        for (int i = 0; i < 40; i++) many.append(i == 0 ? "" : ",").append("k").append(i).append(":").append(i % 3 == 0 ? "{a:" + i + ",b:\"s\"}" : "" + i);
        many.append(",a:5,b:\"x\",DataVersion:77}");
        snbt.add(many.toString());
        List<CompoundTag> tags = new ArrayList<>();
        for (String s : snbt) tags.add(TagParser.parseCompoundFully(s));
        // wrapper-shaped compounds ({"": x}) cannot be written as SNBT keys
        CompoundTag w = new CompoundTag();
        CompoundTag w1 = new CompoundTag(); w1.putInt("", 1);
        CompoundTag w2 = new CompoundTag(); w2.putInt("", 2);
        CompoundTag w3 = new CompoundTag(); w3.putInt("x", 1);
        CompoundTag w4 = new CompoundTag(); w4.put("", w1.copy());
        ListTag l1 = new ListTag(); l1.add(w1.copy()); l1.add(w2.copy());
        ListTag l2 = new ListTag(); l2.add(w1.copy()); l2.add(w3.copy());
        ListTag l3 = new ListTag(); l3.add(w4.copy());
        w.put("a", w1.copy()); w.put("b", l1); w.put("c", l2); w.put("d", l3); w.putInt("DataVersion", 5);
        tags.add(w);
        for (CompoundTag ct : tags) {
            byte[] b = named(ct);
            inputs.add(hex(b));
            inputs.add(hex(Arrays.copyOf(b, b.length - 1)));
            inputs.add(hex(Arrays.copyOf(b, b.length / 2)));
            inputs.add(hex(Arrays.copyOf(b, b.length / 3)));
            inputs.add(hex(Arrays.copyOf(b, 2 * b.length / 3)));
        }

        // ---- hand-made, raw bytes
        String[] hand = {
            "00", "0d", "ff0000", "0d0000", "01", "0100", "0100007f", "08000000", "0800000003616263", "0800000005616263",
            "0a0000 0d000161 00",
            "0a0000 09000161 0d 00000000 00",
            "0a0000 09000161 0d 00000001 00",
            "0a0000 09000161 00 00000003 00",
            "0a0000 09000161 00 00000000 00",
            "0a0000 09000161 03 ffffffff 00",
            "0a0000 07000161 ffffffff 00",
            "0a0000 07000161 00000003 010203 00",
            "0a0000 0b000161 7fffffff 00",
            "0a0000 0c000161 20000000 00",
            "0a0000 0b000161 00000002 0000000100000002 00",
            "0a0000 09000161 0a 00000002 00 00 00",
            "0a0000 08000161 0005 6162",
            "0a0000 08000161 0003 616263 00",
            "0a0000 09000161 08 00000002 00016100016200 00",
            "0a0000 01000161 05 03000162 00000007 00",
            "0a0000 03000161 00000001 03000161 00000002 03000162 00000003 00",
            "0a0000 0a000161 03000161 00000001 00 03000162 00000003 00",
            "0a0000 0a000161 0a000161 03000162 00000001 00 00 0a000161 00 00",
            "0a0000 09000161 09 00000002 03 00000001 00000007 01 00000001 05 00",
            "0a0000 03000161 00000001 0a000162 0d000161 00 00",
            "0a0000 09000161 04 7fffffff 00",
            "0a0000 09000161 07 00000002 00000001ff 00000000 00",
            "0a0000 080001610003c0 80 00 00",
            "0a0000 08000161 0002 c080 00",
            "0a0000 080001c1 0000 00",
            "0a0000 03000161 00000001 00 ff",
            "0a0000 03000161 00000001 ff",
        };
        for (String h : hand) inputs.add(h.replace(" ", ""));
        StringBuilder deep = new StringBuilder("0a0000");
        for (int i = 0; i < 520; i++) deep.append("0a000161");
        for (int i = 0; i < 521; i++) deep.append("00");
        inputs.add(deep.toString());
        StringBuilder deepList = new StringBuilder("090000");
        for (int i = 0; i < 520; i++) deepList.append("0900000001");
        deepList.append("0000000000");
        inputs.add(deepList.toString());

        int rows = 0;
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            for (String h : inputs) {
                out.println("P\t" + h + "\t" + String.join("\t", cells(unhex(h))));
                rows++;
            }
        }
        System.err.println("wrote " + rows + " cases");
    }
}
