import net.minecraft.nbt.*;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;

/**
 * Generates snbt_golden.tsv: one line per input, tab separated, fields escaped with esc():
 *   input, parse-status (O|E), then for O: compact, pretty, anyTag-hex, namedCompound-hex ("-" if not compound);
 *   for E: message, cursor. Final field: compound-mode result: "O" or the error message.
 * Usage: scripts/vanilla-java.sh -o crates/mc-nbt/tests/fixtures/snbt_golden.tsv crates/mc-nbt/tests/fixtures/SnbtOracle.java crates/mc-nbt/tests/fixtures/snbt_golden.tsv
 * Output: crates/mc-nbt/tests/fixtures/snbt_golden.tsv
 */
public class SnbtOracle {
    static boolean lone;

    static String esc(String s) {
        if (hasLone(s)) lone = true;
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\') b.append("\\\\");
            else if (c == '\n') b.append("\\n");
            else if (c == '\t') b.append("\\t");
            else if (c == '\r') b.append("\\r");
            else if (c < 0x20 || (c >= 0xD800 && c <= 0xDFFF && !(Character.isHighSurrogate(c) && i + 1 < s.length() && Character.isLowSurrogate(s.charAt(i + 1))) && !(Character.isLowSurrogate(c) && i > 0 && Character.isHighSurrogate(s.charAt(i - 1)))))
                b.append(String.format("\\x%02X", (int) c));
            else b.append(c);
        }
        return b.toString();
    }

    static boolean hasLone(String s) {
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (Character.isHighSurrogate(c)) { if (i + 1 < s.length() && Character.isLowSurrogate(s.charAt(i + 1))) i++; else return true; }
            else if (Character.isLowSurrogate(c)) return true;
        }
        return false;
    }

    static String hex(byte[] b) {
        StringBuilder sb = new StringBuilder();
        for (byte x : b) sb.append(String.format("%02x", x));
        return sb.toString();
    }

    static final String[] FRAGS = {
        "{", "}", "[", "]", ",", ":", ";", "B;", "I;", "L;", "'", "\"", "\\", "\\n", "\\x41", "\\u00e9", "\\U0001F600", "\\u0041",
        "\\N{LATIN SMALL LETTER A}", "\\N{GREEK SMALL LETTER ALPHA}", "\\N{nope}", "\\xZZ", "\\u12", "\\U00110000", "\\UFFFFFFFF", "\\q",
        "a", "foo", "true", "False", "TRUE", "bool(", "uuid(", "bool", "uuid", ")", "(", "1", "0", "00", "12", "-", "+", ".", "e", "E", "5", "1.5",
        "0x", "0b", "0xFF", "0b101", "0X1f", "_", "1_0", "b", "B", "s", "S", "i", "I", "l", "L", "f", "F", "d", "D", "ub", "sb", "us", "ss", "ui", "si", "ul", "sl", "u", "U",
        " ", "\t", "\n", "é", "😀", "1e5", "9999999999", "256", "128", "-129", "255", "65535", "65536", "4294967295", "4294967296", "123456789012345678901234567890",
        "18446744073709551615", "18446744073709551616", "9223372036854775807", "9223372036854775808", "-9223372036854775808",
        "NaN", "Infinity", "1e999", "1e-999", "key", "\"k\"", "'k'", "{}", "[]", "[B;]", "[I;1,2]", "[L;1L]", "[B;1b,2]", "{a:1}", "{a:{b:2}}", "[1,2]", "[{},{}]", "[[]]",
        "1.", ".5", "1.e2", "-.5e-3", "+1", "--1", "0.5f", "1d", "2.5D", "1e3f", "1_000.5", "1._5", "1.5_", "uuid(\"123e4567-e89b-12d3-a456-426614174000\")", "uuid(\"1-1-1-1-1\")",
        "bool(1)", "bool(0.0)", "bool(\"x\")", "bool(true)", "bool(1,2)", "foo(1)", "x()", ",,", "::", "{,}", "{a:}", "{:1}", "{'':1}", "{\"\":1}", "{a b:1}", "{a:1 b:2}",
        "\u00a0", "\u2028", "\u3000", "\u000b", "\u001c", "\u0000", "%", "%s", "#", "@", "\\\\", "\\'", "\\\"", "\\b", "\\s", "\\t", "\\f", "\\r",
        "0e0", "00.5", "0.0", "-0.0", "-0", "-0b1", "0b", "0x_1", "0x1_", "0b1_", "1__2", "123b", "1000s", "-129b", "0xFFFFFFFFFFFFFFFFl", "0xFFFFFFFFFFFFFFFFFl", "0b11111111111111111111111111111111111111111111111111111111111111111",
    };

    static Random rnd = new Random(20260303L);

    static String randomFrag() { return FRAGS[rnd.nextInt(FRAGS.length)]; }

    static String fuzz() {
        int n = 1 + rnd.nextInt(10);
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < n; i++) b.append(randomFrag());
        return b.toString();
    }

    static String randString() {
        String[] pool = {"a", "b", "hello", "w x", "it's", "say \"hi\"", "both ' and \"", "tab\there", "nl\nx", "\\", "é", "日本", "😀", "", "{}", "true", "123", "a.b-c+d", "_x", "A1", "ctl\u0001", "\u007f", "\u0000x"};
        return pool[rnd.nextInt(pool.length)];
    }

    static String snbtString(String s) { return StringTag.quoteAndEscape(s); }

    static String randNumber() {
        switch (rnd.nextInt(14)) {
            case 0: return Integer.toString(rnd.nextInt()) ;
            case 1: return rnd.nextInt(256) + "b";
            case 2: return (rnd.nextInt(65536) - 32768) + "s";
            case 3: return rnd.nextLong() + "L";
            case 4: { double d = Double.longBitsToDouble(rnd.nextLong()); if (!Double.isFinite(d)) d = 1.5; return Double.toString(d) + "d"; }
            case 5: { float f = Float.intBitsToFloat(rnd.nextInt()); if (!Float.isFinite(f)) f = 2.5f; return Float.toString(f) + "f"; }
            case 6: { double d = (rnd.nextDouble() - 0.5) * Math.pow(10, rnd.nextInt(30) - 15); return Double.toString(d); }
            case 7: return "0x" + Long.toHexString(rnd.nextLong() >>> rnd.nextInt(64)) + (rnd.nextBoolean() ? "" : "L");
            case 8: return "0b" + Integer.toBinaryString(rnd.nextInt(1 << 10));
            case 9: return (rnd.nextBoolean() ? "-" : "") + rnd.nextInt(1000) + "_" + (100 + rnd.nextInt(900));
            case 10: { String[] suf = {"ub", "sb", "us", "ss", "ui", "si", "ul", "sl", "UB", "SS", "i", "I", "S", "B", "l"}; return rnd.nextInt(300) + suf[rnd.nextInt(suf.length)]; }
            case 11: { float f = (rnd.nextFloat() - 0.5f) * (float) Math.pow(10, rnd.nextInt(20) - 10); return Float.toString(f) + "f"; }
            case 12: return (rnd.nextInt(100) - 50) + "." + rnd.nextInt(1000) + (rnd.nextBoolean() ? "e" + (rnd.nextInt(20) - 10) : "") + (rnd.nextBoolean() ? "d" : "");
            default: return rnd.nextBoolean() ? "true" : "false";
        }
    }

    static String structured(int depth) {
        int k = depth <= 0 ? rnd.nextInt(3) : rnd.nextInt(9);
        switch (k) {
            case 0: return randNumber();
            case 1: return snbtString(randString());
            case 2: return rnd.nextBoolean() ? randNumber() : randString().matches("[A-Za-z_][A-Za-z0-9_]*") ? randString() : randNumber();
            case 3: case 4: {
                int n = rnd.nextInt(5);
                StringBuilder b = new StringBuilder("{");
                for (int i = 0; i < n; i++) {
                    if (i > 0) b.append(",");
                    String key = rnd.nextInt(4) == 0 ? randString() : "k" + rnd.nextInt(30);
                    if (key.isEmpty()) key = "e";
                    b.append(snbtString(key)).append(":").append(structured(depth - 1));
                }
                return b.append("}").toString();
            }
            case 5: case 6: {
                int n = rnd.nextInt(5);
                StringBuilder b = new StringBuilder("[");
                boolean hetero = rnd.nextInt(3) == 0;
                String shape = structured(depth - 1);
                for (int i = 0; i < n; i++) {
                    if (i > 0) b.append(",");
                    b.append(hetero ? structured(depth - 1) : (rnd.nextBoolean() ? shape : structured(depth - 1)));
                }
                return b.append("]").toString();
            }
            case 7: {
                String[] p = {"B", "I", "L"};
                String pre = p[rnd.nextInt(3)];
                int n = rnd.nextInt(5);
                StringBuilder b = new StringBuilder("[" + pre + ";");
                for (int i = 0; i < n; i++) {
                    if (i > 0) b.append(",");
                    b.append(rnd.nextInt(200) - 100);
                    if (rnd.nextInt(4) == 0) b.append(pre.equals("B") ? "b" : pre.equals("L") ? "L" : "s");
                }
                return b.append("]").toString();
            }
            default: return "[" + (rnd.nextBoolean() ? "" : snbtString(randString())) + "]";
        }
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        List<String> inputs = new ArrayList<>();
        // hand-written corpus
        String[] hand = {
            "{}", "[]", "1", "1b", "{a:1}", "{a:1,b:2,}", "[1,2,3,]", "[1,2,]", "[,]", "{a:1b,b:[1,2,3],c:0x1F,d:'x\\ny'}",
            "{Name:\"minecraft:stone\",Properties:{facing:\"north\"}}", "{display:{Name:'{\"text\":\"x\"}'}}",
            "{Items:[{id:\"minecraft:stone\",count:1},{id:\"minecraft:dirt\",count:2b}]}",
            "[1,2.5,\"x\",{a:1},[1]]", "[1b,2b,3b]", "[B;1,2,3]", "[B;1b,2b,-1b]", "[B;0xFF]", "[B;255]", "[I;1,2,3]", "[I;1b,2s,3]", "[L;1,2L,3b]", "[L;1.5]", "[I;1L]", "[B;]", "[I;]", "[L;]",
            "{a:1,a:2}", "{Aa:1,BB:2,Aa:3}", "{BB:1,Aa:2,BB:3}", "  {  a  :  1  ,  b  :  2  }  ", "{\n\ta: 1,\n\tb: 2\n}", "{a:1} x", "{a:1}}", "{a:1", "{a:", "{", "}", "", " ", "x", "'x", "\"x", "'x\\", "\"\\u00e",
            "uuid(\"123e4567-e89b-12d3-a456-426614174000\")", "uuid(\"00000000-0000-0000-0000-000000000001\")", "uuid('-1-1-1-1')", "uuid(1)", "uuid(\"x\")", "bool(5)", "bool(0)", "bool(1.5)", "bool(0.0f)", "bool(\"s\")", "bool()", "bool(1,2)",
            "bool(true)", "bool(false)", "[bool(1),bool(0)]", "{a:uuid(\"123e4567-e89b-12d3-a456-426614174000\")}",
            "\"\\uD83D\\uDE00\"", "'\\N{SMILING FACE WITH OPEN MOUTH}'", "'\\N{LATIN CAPITAL LETTER A}'", "'\\N{latin small letter b}'", "'\\N{ZERO WIDTH SPACE}'", "'\\N{CJK UNIFIED IDEOGRAPH-4E00}'", "'\\N{HANGUL SYLLABLE GA}'", "'\\N{BASIC LATIN 0041}'", "'\\N{}'",
            "'a\"b'", "\"a'b\"", "'a\\'b'", "\"a\\\"b\"", "'\\s'", "'\\x00'", "\"\\x7f\"", "'\\0'", "{a:'\\b\\t\\n\\f\\r'}",
            "1e5", "1E5", "1e+5", "1e-5", "1.5e3f", "1.5e3d", "1f", "1d", ".5f", "5.f", "5.", "-5.", "+5.5", "1_0.0_1", "0.1f", "16777217f", "3.4028235e38f", "3.4028236e38f", "1.7976931348623157e308", "1.7976931348623159e308", "4.9e-324", "1e-400", "1.4e-45f", "1e-50f",
            "0", "-0", "+0", "00", "007", "0_7", "0b", "0b1", "0b102", "0x", "0xg", "0x1G", "0X0", "0b1010_1010", "0xCAFE_BABE", "0xFFFFFFFF", "0xFFFFFFFFF", "0xFFFFFFFFFFFFFFFF", "0xFFFFFFFFFFFFFFFFF", "0x7FFFFFFF", "0x80000000", "-0x1", "-0x1s", "-0b1",
            "2147483647", "2147483648", "-2147483648", "-2147483649", "9223372036854775807L", "9223372036854775808L", "-9223372036854775808L", "-9223372036854775809L", "127b", "128b", "-128b", "-129b", "255ub", "256ub", "-1ub", "255sb", "32767s", "32768s", "65535us", "65536us", "4294967295ui", "4294967296ui", "18446744073709551615ul", "18446744073709551616ul",
            "1sb", "1ss", "1si", "1sl", "1ub", "1us", "1ui", "1ul", "1usb", "1bs", "1B", "1S", "1I", "1L", "1l", "1i", "1ibs", "1u", "1s1",
            "1.5b", "1.5s", "1.5L", "1.5i", "1.5ub", "1e5b", "1e5L",
            "123456789012345678901234567890", "123456789012345678901234567890L", "1__0", "_1", "1_", "1_b", "1_.5", "1._5",
            "NaN", "Infinity", "-Infinity", "1e999", "-1e999", "1e999f", "1e39f",
            "[B;1,2,3", "[B", "[B;", "[X;1]", "[B 1]", "[B;1;2]", "[I;1,,2]", "[1 2]", "[1,2", "[,1]",
            "{a:1,,b:2}", "{,a:1}", "{a:1,b}", "{a 1}", "{\"a\":1}", "{'a':1}", "{a:1 b:2}", "{1:1}", "{-a:1}", "{+a:1}", "{a-b:1}", "{a.b:1}", "{a+b:1}", "{1a:1}", "{.a:1}", "{é:1}",
            "foo", "foo bar", "foo(1)", "foo()", "foo(", "bool(1", "bool(1)x", "bool 1", "a(b)", "1(2)",
            "true", "false", "True", "FALSE", "tRuE", "truee", "nul", "null",
            "\u00a0{}", "{\u00a0}", "\u3000{a:1}\u3000", "{a:1}\u2028", "{a:1}\u0000", "{a:\u000b1}",
            "[[[[[[[[[[1]]]]]]]]]]", "{a:{b:{c:{d:{e:{f:{g:{}}}}}}}}",
            "[{},{a:1},{b:2}]", "[1,{a:1}]", "[{a:1},1]", "[[1],[2,3]]", "[[1],2]", "[{\"\":1}]", "[{\"\":1,a:2}]", "{\"\":1}",
            "{size:[1,2,3],data:[{pos:[0,0,0],state:0}],palette:[{Name:\"minecraft:stone\"}],entities:[],DataVersion:100,author:\"x\"}",
            "{z:1,a:2,B:3,\"\u00e9\":4,\"\uD83D\uDE00\":5,\"\uFB01\":6}",
            "{\"a b\":1,\"true\":2,\"False\":3,\"1a\":4,_x:5,'it\\'s':6,\"q\\\"\":7}", "{a:\"%s\"}", "{\"%s\":1}", "'%'",
        };
        inputs.addAll(Arrays.asList(hand));
        for (int i = 0; i < 12000; i++) inputs.add(fuzz());
        for (int i = 0; i < 4000; i++) inputs.add(structured(3));
        for (int i = 0; i < 600; i++) inputs.add("{" + (rnd.nextBoolean() ? "a" : "k") + ":" + structured(2) + "}");
        // mutate structured valid inputs (error positions inside deep structures)
        for (int i = 0; i < 3000; i++) {
            String s = structured(3);
            if (s.length() < 2) continue;
            int p = rnd.nextInt(s.length());
            String m;
            switch (rnd.nextInt(3)) {
                case 0: m = s.substring(0, p); break;
                case 1: m = s.substring(0, p) + randomFrag() + s.substring(p); break;
                default: m = s.substring(0, p) + s.substring(Math.min(s.length(), p + 1 + rnd.nextInt(3))); break;
            }
            inputs.add(m);
        }
        // exact-number fuzz around integer limits
        for (int i = 0; i < 3000; i++) {
            String[] pre = {"", "-", "+", "0x", "0b", "-0x", "0X", "-0b"};
            String[] suf = {"", "b", "s", "i", "l", "L", "ub", "us", "ui", "ul", "sb", "ss", "si", "sl", "B", "UL", "f", "d"};
            int len = 1 + rnd.nextInt(24);
            StringBuilder d = new StringBuilder();
            String p = pre[rnd.nextInt(pre.length)];
            String alpha = p.contains("b") || p.contains("B") ? "01" : p.contains("x") || p.contains("X") ? "0123456789abcdefABCDEF" : "0123456789";
            for (int j = 0; j < len; j++) { d.append(alpha.charAt(rnd.nextInt(alpha.length()))); if (rnd.nextInt(12) == 0) d.append('_'); }
            inputs.add(p + d + suf[rnd.nextInt(suf.length)]);
            if (rnd.nextInt(3) == 0) inputs.add("[" + (rnd.nextBoolean() ? "B" : rnd.nextBoolean() ? "I" : "L") + ";" + p + d + suf[rnd.nextInt(suf.length)] + "]");
        }

        // \N{...} lookups: real names, block-hex fallback names, case/space variations, mutations
        for (int i = 0; i < 2500; i++) {
            int cp = rnd.nextInt(5) == 0 ? rnd.nextInt(0x110000) : rnd.nextInt(0x30000);
            String name = Character.getName(cp);
            if (name == null) continue;
            switch (rnd.nextInt(5)) {
                case 0: name = name.toLowerCase(); break;
                case 1: name = "  " + name + " "; break;
                case 2: { int p = rnd.nextInt(name.length()); name = name.substring(0, p) + (char) ('A' + rnd.nextInt(26)) + name.substring(p + 1); break; }
                case 3: name = name.replace(' ', '-'); break;
                default: break;
            }
            inputs.add("'\\N{" + name + "}'");
        }
        for (String n : new String[]{"NULL", "LF", "LINE FEED", "SPACE", "DELETE", "PRIVATE USE AREA E000", "PRIVATE USE AREA e000", "PRIVATE USE AREA E00", "BASIC LATIN 41", "BASIC LATIN 0041", "HIGH SURROGATES D800", "LOW SURROGATES DC00",
                "HANGUL SYLLABLE GA", "CJK UNIFIED IDEOGRAPH-4E00", "CJK UNIFIED IDEOGRAPHS 4E00", "CJK COMPATIBILITY IDEOGRAPH-F900", "GREEK AND COPTIC 378", "GREEK AND COPTIC 0378", "-", " ", "A", "EURO SIGN", "EURO  SIGN", "euro sign", "TAG LATIN SMALL LETTER A", "VARIATION SELECTOR-1", "VS1", "BYTE ORDER MARK", "ZWSP", "NO-BREAK SPACE", "NBSP", "PILE OF POO", "BASIC LATIN -41", "BASIC LATIN +41", "SUPPLEMENTARY PRIVATE USE AREA-A 10FFFD", "SUPPLEMENTARY PRIVATE USE AREA-A 10FFFE", "TANGUT 17000", "KATAKANA LETTER SMALL KU", "BASIC LATIN 80000000", "BASIC LATIN FFFFFFFF"})
            inputs.add("'\\N{" + n + "}'");

        // uuid(...) parsing (java.util.UUID.fromString is lenient)
        String uuidAlpha = "0123456789abcdefABCDEFgG+- ";
        for (int i = 0; i < 1500; i++) {
            int len = rnd.nextInt(40);
            StringBuilder u = new StringBuilder();
            if (rnd.nextBoolean()) {
                String full = "123e4567-e89b-12d3-a456-426614174000";
                u.append(full);
                for (int k = rnd.nextInt(3); k > 0 && u.length() > 0; k--) u.setCharAt(rnd.nextInt(u.length()), uuidAlpha.charAt(rnd.nextInt(uuidAlpha.length())));
                if (rnd.nextInt(4) == 0) u.deleteCharAt(rnd.nextInt(u.length()));
            } else {
                for (int j = 0; j < len; j++) u.append(rnd.nextInt(5) == 0 ? '-' : uuidAlpha.charAt(rnd.nextInt(uuidAlpha.length() - 1)));
            }
            inputs.add("uuid(\"" + u + "\")");
        }
        for (String u : new String[]{"1-1-1-1-1", "0-0-0-0-0", "ffffffff-ffff-ffff-ffff-ffffffffffff", "FFFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF", "+1-+2-+3-+4-+5", "-1-1-1-1-1", "1-1-1-1-", "1-1-1-1", "1-1-1-1-1-1", "", "-", "----", "1--1-1-1", "123456789-1-1-1-1", "1-12345-1-1-1", "1-1-1-12345-1", "1-1-1-1-123456789abc", "1-1-1-1-1234567890abc", "8000000000000000-0-0-0-0", "x-1-1-1-1"})
            inputs.add("uuid(\"" + u + "\")");

        // inputs containing lone surrogates cannot be represented as Rust &str
        inputs.removeIf(SnbtOracle::hasLone);

        TagParser<Tag> parser = TagParser.create(NbtOps.INSTANCE);
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            for (String in : inputs) {
                lone = false;
                StringBuilder line = new StringBuilder(esc(in)).append('\t');
                try {
                    Tag t = parser.parseFully(in);
                    ByteArrayOutputStream any = new ByteArrayOutputStream();
                    NbtIo.writeAnyTag(t, new DataOutputStream(any));
                    String named = "-";
                    if (t instanceof CompoundTag c) {
                        ByteArrayOutputStream n = new ByteArrayOutputStream();
                        NbtIo.write(c, new DataOutputStream(n));
                        named = hex(n.toByteArray());
                    }
                    line.append("O\t").append(esc(t.toString())).append('\t').append(esc(new SnbtPrinterTagVisitor().visit(t))).append('\t').append(hex(any.toByteArray())).append('\t').append(named);
                } catch (CommandSyntaxException e) {
                    line.append("E\t").append(esc(e.getMessage())).append('\t').append(e.getCursor()).append("\t-\t-");
                } catch (Exception e) {
                    line.append("X\t").append(esc(e.toString())).append("\t-\t-\t-");
                }
                line.append('\t');
                try { TagParser.parseCompoundFully(in); line.append("O"); }
                catch (CommandSyntaxException e) { line.append("E").append(esc(e.getMessage())); }
                catch (Exception e) { line.append("X").append(esc(e.toString())); }
                if (!lone) out.println(line);
            }
        }
        System.err.println("wrote " + inputs.size() + " cases");
    }
}
