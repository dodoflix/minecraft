import net.minecraft.nbt.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/**
 * Generates binary_golden.tsv from the real NbtIo/NbtAccounter:
 *   "S\t<name>\t<hex>"                       tag-building scenarios (recipes mirrored in tests/golden/binary_golden.rs)
 *   "R\t<hex>\t<usage>\t<r1>\t<r2>\t<r3>\t<r4>"  reads of every OK case of snbt_golden.tsv (+ hand cases):
 *        usage with unlimited quota, then results with quota usage-1, usage/2, truncated-1, truncated-half
 *   "H\t<hex>\t<result>"                     hand-made malformed inputs read with the default quota
 * Run: scripts/vanilla-java.sh crates/mc-nbt/tests/fixtures/BinaryOracle.java crates/mc-nbt/tests/fixtures/binary_golden.tsv crates/mc-nbt/tests/fixtures/snbt_golden.tsv
 */
public class BinaryOracle {
    static String hex(byte[] b) {
        StringBuilder sb = new StringBuilder();
        for (byte x : b) sb.append(String.format("%02x", x));
        return sb.toString();
    }

    static byte[] unhex(String h) {
        byte[] b = new byte[h.length() / 2];
        for (int i = 0; i < b.length; i++) b[i] = (byte) Integer.parseInt(h.substring(2 * i, 2 * i + 2), 16);
        return b;
    }

    static byte[] named(CompoundTag c) throws IOException {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        NbtIo.write(c, new DataOutputStream(o));
        return o.toByteArray();
    }

    static String result(byte[] bytes, NbtAccounter acc) {
        try {
            Tag t = NbtIo.readAnyTag(new DataInputStream(new ByteArrayInputStream(bytes)), acc);
            return "OK";
        } catch (NbtAccounterException | NbtFormatException | IllegalArgumentException e) {
            return e.getClass().getSimpleName() + ":" + e.getMessage();
        } catch (EOFException e) {
            return "EOF";
        } catch (ReportedNbtException e) {
            Throwable c = e.getCause();
            return "Reported:" + (c instanceof EOFException ? "EOF" : c.getClass().getSimpleName() + ":" + c.getMessage());
        } catch (IOException e) {
            return "IO:" + e.getClass().getSimpleName() + ":" + e.getMessage();
        } catch (Exception e) {
            return "X:" + e;
        }
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        net.minecraft.SharedConstants.IS_RUNNING_IN_IDE = false; // NbtIo string fallback would otherwise pause
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            // ---- scenarios
            CompoundTag s1 = new CompoundTag();
            for (int i = 0; i < 40; i++) s1.putInt("k" + i, i);
            out.println("S\torder40\t" + hex(named(s1)));
            CompoundTag s2 = new CompoundTag();
            for (int i = 0; i < 13; i++) s2.putInt("key_" + i, i);
            out.println("S\torder13\t" + hex(named(s2)));
            String[] coll = {"Aa", "BB", "AaAa", "BBBB", "AaBB", "BBAa", "C#", "Bb", "Aa1", "BB1"};
            CompoundTag s3 = new CompoundTag();
            for (String k : coll) s3.putString(k, k);
            out.println("S\tcollide\t" + hex(named(s3)));
            CompoundTag s3b = new CompoundTag();
            for (int i = coll.length - 1; i >= 0; i--) s3b.putString(coll[i], coll[i]);
            out.println("S\tcollide_rev\t" + hex(named(s3b)));
            CompoundTag s4 = new CompoundTag();
            for (int i = 0; i < 30; i++) s4.putInt("n" + i, i);
            for (int i = 0; i < 30; i += 3) s4.remove("n" + i);
            for (int i = 0; i < 30; i += 4) s4.remove("n" + i);
            out.println("S\tremoved\t" + hex(named(s4)));
            out.println("S\tcopy_of_removed\t" + hex(named(s4.copy())));
            CompoundTag s4b = s4.copy();
            s4b.putString("Aa", "x"); s4b.putString("BB", "y");
            out.println("S\tcopy_then_put\t" + hex(named(s4b)));
            out.println("S\tops_remove\t" + hex(named((CompoundTag) NbtOps.INSTANCE.remove(s1, "k5"))));
            CompoundTag s6 = new CompoundTag();
            s6.putInt("a", 1);
            CompoundTag s6c = (CompoundTag) NbtOps.INSTANCE.remove(s6, "zz");
            for (int i = 0; i < 10; i++) s6c.putInt("q" + i, i);
            out.println("S\tshallow_grow\t" + hex(named(s6c)));
            CompoundTag s7 = new CompoundTag();
            for (int i = 0; i < 20; i++) s7.putInt("x" + i, i);
            CompoundTag s7o = new CompoundTag();
            s7o.putInt("x3", 99); s7o.putInt("new1", 1); s7o.putInt("new2", 2);
            CompoundTag sub = new CompoundTag(); sub.putInt("deep", 1); sub.putInt("deeper", 2);
            s7.put("sub", sub);
            CompoundTag subo = new CompoundTag(); subo.putInt("deep", 5); subo.putInt("other", 3);
            s7o.put("sub", subo);
            s7.merge(s7o);
            out.println("S\tmerge\t" + hex(named(s7)));
            CompoundTag s8 = new CompoundTag();
            String[] strs = {"", "a", "\u0000", "a\u0000b", "\u0001", "\u007f", "\u0080", "\u07ff", "\u0800", "\uffff", "\uD83D\uDE00", "é日本語😀", "\ud7ff\ue000"};
            for (int i = 0; i < strs.length; i++) { s8.putString("s" + i, strs[i]); s8.putString(strs[i] + "key" + i, "v"); }
            out.println("S\tstrings\t" + hex(named(s8)));
            CompoundTag s9 = new CompoundTag();
            s9.putString("long", "x".repeat(30000) + "\u0800".repeat(30000));
            s9.putString("ok", "y".repeat(65535));
            s9.putString("x".repeat(70000), "v");
            out.println("S\tlongstring\t" + hex(named(s9)));
            CompoundTag s10 = new CompoundTag();
            ListTag mixed = new ListTag(); mixed.add(IntTag.valueOf(1)); mixed.add(StringTag.valueOf("a"));
            CompoundTag c1 = new CompoundTag(); c1.putInt("x", 1); mixed.add(c1);
            CompoundTag c2 = new CompoundTag(); c2.putInt("", 2); mixed.add(c2);
            s10.put("mixed", mixed);
            ListTag comps = new ListTag();
            CompoundTag w1 = new CompoundTag(); w1.putInt("", 1); comps.add(w1);
            CompoundTag w2 = new CompoundTag(); w2.putInt("", 1); w2.putInt("b", 1); comps.add(w2);
            comps.add(new CompoundTag());
            s10.put("comps", comps);
            ListTag lists = new ListTag(); ListTag l1 = new ListTag(); l1.add(ByteTag.valueOf((byte) 1)); lists.add(l1); lists.add(new ListTag());
            ListTag l2 = new ListTag(); l2.add(ShortTag.valueOf((short) 2)); lists.add(l2);
            s10.put("lists", lists);
            ListTag withEnd = new ListTag(); withEnd.add(EndTag.INSTANCE);
            s10.put("withEnd", withEnd);
            ListTag withEnd2 = new ListTag(); withEnd2.add(EndTag.INSTANCE); withEnd2.add(IntTag.valueOf(1));
            s10.put("withEnd2", withEnd2);
            ListTag hetNum = new ListTag(); hetNum.add(IntTag.valueOf(1)); hetNum.add(LongTag.valueOf(2L)); hetNum.add(DoubleTag.valueOf(0.5)); hetNum.add(new IntArrayTag(new int[]{1, 2}));
            s10.put("hetNum", hetNum);
            out.println("S\tlistwrap\t" + hex(named(s10)));
            CompoundTag s11 = new CompoundTag();
            s11.putDouble("negzero", -0.0); s11.putFloat("negzerof", -0.0f);
            s11.putDouble("nan", Double.NaN); s11.putFloat("nanf", Float.NaN);
            s11.putDouble("nan2", Double.longBitsToDouble(0x7ff8000000000123L)); s11.putFloat("nanf2", Float.intBitsToFloat(0x7fc00123));
            s11.putDouble("inf", Double.POSITIVE_INFINITY); s11.putDouble("min", Double.MIN_VALUE); s11.putFloat("fmax", Float.MAX_VALUE);
            s11.putByteArray("ba", new byte[]{1, -2, 3}); s11.putIntArray("ia", new int[]{1, -2, Integer.MIN_VALUE}); s11.putLongArray("la", new long[]{1L, Long.MIN_VALUE});
            s11.putByteArray("empty", new byte[0]);
            s11.putBoolean("t", true); s11.putByte("b", (byte) -5); s11.putShort("sh", (short) -300); s11.putLong("l", -1L);
            out.println("S\tmisc\t" + hex(named(s11)));
            // reading float NaN payload / negative zero back: toString of what was read
            ByteArrayOutputStream o2 = new ByteArrayOutputStream();
            DataOutputStream d2 = new DataOutputStream(o2);
            d2.writeByte(10); d2.writeUTF("");
            d2.writeByte(6); d2.writeUTF("z"); d2.writeLong(0x8000000000000000L);
            d2.writeByte(5); d2.writeUTF("f"); d2.writeInt(0x80000000);
            d2.writeByte(0);
            CompoundTag rz = NbtIo.read(new DataInputStream(new ByteArrayInputStream(o2.toByteArray())));
            out.println("S\tread_negzero\t" + hex(named(rz)));

            // ---- accounter / truncation reads over the snbt golden corpus
            List<String> hexes = new ArrayList<>();
            for (String line : Files.readAllLines(Paths.get(args[1]), StandardCharsets.UTF_8)) {
                String[] f = line.split("\t", -1);
                if (f[1].equals("O")) hexes.add(f[4]);
            }
            // every 16th (CrashReport creation per truncated read is slow), plus the shortest ones
            Set<String> seen = new LinkedHashSet<>();
            for (int i = 0; i < hexes.size(); i++) if (i % 16 == 0 || hexes.get(i).length() < 24) seen.add(hexes.get(i));
            for (String h : seen) {
                byte[] b = unhex(h);
                NbtAccounter all = NbtAccounter.unlimitedHeap();
                NbtIo.readAnyTag(new DataInputStream(new ByteArrayInputStream(b)), all);
                long usage = all.getUsage();
                String r1 = result(b, new NbtAccounter(Math.max(0, usage - 1), 512));
                String r2 = result(b, new NbtAccounter(usage / 2, 512));
                String r3 = result(Arrays.copyOf(b, Math.max(0, b.length - 1)), NbtAccounter.unlimitedHeap());
                String r4 = result(Arrays.copyOf(b, b.length / 2), NbtAccounter.unlimitedHeap());
                out.println("R\t" + h + "\t" + usage + "\t" + r1 + "\t" + r2 + "\t" + r3 + "\t" + r4);
            }
            // ---- hand-made
            List<String> hand = new ArrayList<>();
            hand.add("0d");                           // invalid tag id 13
            hand.add("ff");                           // invalid id -1
            hand.add("0a0300016100000001");           // truncated compound
            hand.add("090000000000");                 // empty list of end
            hand.add("0900ffffffff");                 // negative list length
            hand.add("090000000001");                 // list type end count 1
            hand.add("090d00000000");                 // list of invalid type, count 0
            hand.add("090d00000001");                 // list of invalid type, count 1
            hand.add("0701ffffff");                   // negative byte array len
            hand.add("0b7fffffff");                   // huge int array
            hand.add("0c7fffffff");
            hand.add("0700000005010203");             // truncated byte array
            hand.add("08000261");                     // truncated string
            hand.add("080002c328");                   // malformed utf
            hand.add("080001c3");                     // partial char
            hand.add("080003e0a000");                 // malformed 3-byte
            hand.add("080001ff");                     // bad lead byte
            hand.add("080001f0");
            hand.add("0a0d0001610000");               // compound with invalid-id element
            hand.add("0a");                           // compound truncated at once
            hand.add("");                             // empty input
            hand.add("0a000130");                     // compound key
            hand.add("0a0100016101" + "00");          // proper compound
            hand.add("0a0100016101" + "0100016201" + "00");
            hand.add("0a0100016101" + "0100016101" + "00"); // duplicate key
            hand.add("08000380c080");                 // C0 80 NUL form
            StringBuilder deepList = new StringBuilder();
            for (int d : new int[]{510, 511, 512, 513, 600}) {
                StringBuilder b = new StringBuilder();
                for (int i = 0; i < d; i++) b.append("0900000001"); // list of list, count 1
                b.append("0100000000");                              // empty? innermost: list of byte count 0
                b.insert(0, "09");
                hand.add(b.toString().substring(2));
                StringBuilder c = new StringBuilder("0a");
                for (int i = 0; i < d; i++) c.append("0a000161");
                c.append("00".repeat(d + 1));
                hand.add(c.toString());
            }
            for (String h : hand) {
                byte[] b = unhex(h);
                out.println("H\t" + h + "\t" + result(b, NbtAccounter.defaultQuota()));
            }
        }
    }
}
