import static com.mojang.brigadier.arguments.BoolArgumentType.*;
import static com.mojang.brigadier.arguments.DoubleArgumentType.*;
import static com.mojang.brigadier.arguments.FloatArgumentType.*;
import static com.mojang.brigadier.arguments.IntegerArgumentType.*;
import static com.mojang.brigadier.arguments.LongArgumentType.*;
import static com.mojang.brigadier.arguments.StringArgumentType.*;

import com.mojang.brigadier.*;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.context.*;
import com.mojang.brigadier.exceptions.*;
import com.mojang.brigadier.suggestion.*;
import com.mojang.brigadier.tree.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/** Generates golden.txt for the mc-brigadier tests: run with scripts/vanilla-java.sh BrigadierGolden.java golden.txt */
public class BrigadierGolden {
    static StringBuilder out = new StringBuilder();
    static StringBuilder events = new StringBuilder();

    static void line(String s) { out.append(s).append('\n'); }

    static com.mojang.brigadier.builder.LiteralArgumentBuilder<Integer> literal(String n) { return com.mojang.brigadier.builder.LiteralArgumentBuilder.literal(n); }
    static <T> com.mojang.brigadier.builder.RequiredArgumentBuilder<Integer, T> argument(String n, ArgumentType<T> t) { return com.mojang.brigadier.builder.RequiredArgumentBuilder.argument(n, t); }

    static CommandDispatcher<Integer> sample() {
        CommandDispatcher<Integer> d = new CommandDispatcher<>();
        LiteralCommandNode<Integer> foo = d.register(literal("foo")
            .then(literal("bar").executes(c -> 1))
            .then(literal("Baz").executes(c -> 2))
            .then(argument("n", integer(0, 100)).executes(c -> getInteger(c, "n"))
                .then(argument("w", word()).executes(c -> getInteger(c, "n") + getString(c, "w").length()))));
        d.register(literal("say").then(argument("msg", greedyString()).executes(c -> getString(c, "msg").length())));
        d.register(literal("give").then(argument("target", word()).then(argument("item", string()).executes(c -> getString(c, "item").length())
            .then(argument("count", integer(1)).executes(c -> getInteger(c, "count"))))));
        d.register(literal("admin").requires(s -> s >= 2).then(literal("stop").executes(c -> 7)).then(literal("op").requires(s -> s >= 3).executes(c -> 8)));
        d.register(literal("tp").then(argument("x", doubleArg(-10, 10)).then(argument("y", doubleArg()).then(argument("z", floatArg(0, 5)).executes(c -> 9)))));
        d.register(literal("flag").then(argument("v", bool()).executes(c -> getBool(c, "v") ? 1 : 0)));
        d.register(literal("big").then(argument("n", longArg(0, 1000)).executes(c -> (int) getLong(c, "n"))));
        d.register(literal("alias").redirect(foo));
        d.register(literal("run").redirect(d.getRoot()));
        d.register(literal("each").fork(d.getRoot(), c -> List.of(c.getSource(), c.getSource() + 10)));
        d.register(literal("as").then(argument("n", integer()).redirect(d.getRoot(), c -> c.getSource() + getInteger(c, "n"))));
        d.register(literal("whoami").executes(c -> c.getSource()));
        d.register(literal("opt").executes(c -> 1).then(literal("x").executes(c -> 2)).then(literal("y")));
        d.register(literal("multi").then(literal("a").executes(c -> 1)).then(literal("b").executes(c -> 1)).then(literal("c")));
        d.register(literal("fail").executes(c -> { throw new SimpleCommandExceptionType(new LiteralMessage("boom")).create(); }));
        d.register(literal("pick").then(argument("c", word()).suggests((c, b) -> {
            b.suggest("beta").suggest("Alpha").suggest("alpha").suggest("Beta").suggest(10).suggest(9, new LiteralMessage("nine")).suggest(100);
            b.suggest("\uD83D\uDE00").suggest("\uFF5E").suggest("\u00DF").suggest("ss").suggest("\u03A3").suggest("\u03C3").suggest("x", new LiteralMessage("tip"));
            return b.buildFuture();
        }).executes(c -> 1)));
        d.register(literal("pick2").then(argument("c", word()).suggests((c, b) -> {
            b.suggest("Alpha").suggest("alpha").suggest("gamma").suggest("Gamma").suggest(3);
            return b.buildFuture();
        })).then(literal("alpha")).then(literal("Alphabet")).then(literal("beta")));
        return d;
    }

    static String path(CommandDispatcher<Integer> d, CommandNode<Integer> n) { return String.join(" ", d.getPath(n)); }

    static String exec(CommandDispatcher<Integer> d, int src, String input) {
        events.setLength(0);
        d.setConsumer((c, ok, r) -> events.append(" [").append(ok).append(',').append(r).append(',').append(c.getSource()).append(']'));
        String res;
        try { res = "OK " + d.execute(input, src); } catch (CommandSyntaxException e) { res = "ERR " + e.getMessage() + " | " + e.getRawMessage().getString() + " | " + e.getCursor(); }
        return res + events;
    }

    static String sugg(Suggestions s, String input) {
        StringBuilder b = new StringBuilder("range=" + s.getRange().getStart() + "-" + s.getRange().getEnd() + " [");
        boolean first = true;
        for (Suggestion x : s.getList()) {
            if (!first) b.append(';');
            first = false;
            b.append(x.getText()).append('/').append(x.getTooltip() == null ? "-" : x.getTooltip().getString()).append('/').append(x instanceof IntegerSuggestion ? "I" : "S").append('/').append(x.apply(input));
        }
        return b.append(']').toString();
    }

    static String norm(String s) { return s.replaceAll("com\\.mojang\\.brigadier\\.arguments\\.BoolArgumentType@[0-9a-f]+", "BoolArgumentType"); }
    static String jd(double v) { return Double.toString(v); }

    public static void main(String[] args) throws Exception {
        CommandDispatcher<Integer> d = sample();

        // ---- usage
        for (boolean restricted : new boolean[] {false, true}) {
            for (int src : new int[] {0, 2, 3}) {
                line("USAGE_ALL " + restricted + " " + src);
                for (String u : d.getAllUsage(d.getRoot(), src, restricted)) line("  " + u);
            }
        }
        for (int src : new int[] {0, 3}) {
            line("SMART_ROOT " + src);
            for (var e : d.getSmartUsage(d.getRoot(), src).entrySet()) line("  " + e.getKey().getName() + " => " + e.getValue());
            for (String p : new String[] {"foo", "admin", "opt", "multi", "give", "pick2"}) {
                line("SMART " + p + " " + src);
                for (var e : d.getSmartUsage(d.findNode(List.of(p)), src).entrySet()) line("  " + e.getKey().getName() + " => " + e.getValue());
            }
        }
        // ---- node toString / path / findNode
        for (String[] p : new String[][] {{"foo"}, {"foo", "n"}, {"foo", "n", "w"}, {"give", "target", "item", "count"}, {"tp", "x"}, {"tp", "x", "y", "z"}, {"flag", "v"}, {"big", "n"}, {"say", "msg"}, {"alias"}, {"nope"}, {"foo", "nope"}}) {
            CommandNode<Integer> n = d.findNode(Arrays.asList(p));
            line("NODE " + String.join(" ", p) + " => " + (n == null ? "null" : norm(n + " | " + n.getUsageText() + " | " + n.getExamples() + " | " + path(d, n))));
        }
        line("PATH_REDIRECT " + path(d, d.getRoot().getChild("alias").getRedirect()));
        line("PATH_FOREIGN " + d.getPath(new LiteralCommandNode<Integer>("zzz", null, c -> true, null, null, false)).size());

        // ---- execute
        String[] inputs = {"", " ", "foo", "foo bar", "foo Bar", "foo Baz", "foo 5", "foo 5 abc", "foo 101", "foo -1", "foo 5.5", "foo abc", "foo 5 abc extra", "foo bar extra",
            "foo  bar", "foo bar ", "foo bar  ", "say", "say ", "say hello world", "say   x", "give a b", "give a \"b c\" 3", "give a 'b c' 0", "give a \"unclosed", "give a \"bad\\q\"",
            "give a b 2147483648", "give a b -3", "give a b 3 4", "admin stop", "admin op", "admin", "tp 1 2 3", "tp 11 0 0", "tp 1 2 6", "tp 1 2 -0.5", "tp 1e2 1 1", "tp .5 -.5 5.",
            "tp 1.2.3 1 1", "tp 1 2", "tp -10.5 1 1", "tp 1 1 1.0000001", "flag true", "flag false", "flag TRUE", "flag", "flag x", "flag \"true\"", "big 1000", "big 1001", "big 99999999999999999999", "big -1",
            "alias bar", "alias 7 x", "alias", "run foo bar", "run run run foo 3", "run", "run bogus", "each foo bar", "each each foo bar", "each foo 99999", "each foo 5 x", "each fail", "each run fail", "as 5 whoami", "as 5 as 6 whoami", "as x whoami", "as 5",
            "whoami", "opt", "opt x", "opt y", "opt z", "multi", "multi a", "multi c", "multi d", "fail", "run fail", "unknown", "FOO bar", "foo\tbar", "\u00E9", "say h\u00E9llo \uD83D\uDE00",
            "foo \uD83D\uDE00", "tp 1 2 \u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9", "tp \uD83D\uDE00\uD83D\uDE00\uD83D\uDE00\uD83D\uDE00\uD83D\uDE00\uD83D\uDE00 1 1",
            "give \uD83D\uDE00\uD83D\uDE00\uD83D\uDE00\uD83D\uDE00\uD83D\uDE00\uD83D\uDE00 \"x\\y\" 1", "foo 12345678901234 bar", "give abcdefghijklmnop \"abcdefghijklmnop\\\\ \\q\" 2", "pick", "pick x"};
        for (int src : new int[] {0, 3}) {
            for (String in : inputs) line("EXEC " + src + "\t" + in + "\t" + exec(d, src, in));
        }

        // ---- suggestions
        String[] suggInputs = {"", "f", "foo", "foo ", "foo b", "foo B", "foo 1", "foo 12 ", "foo 12 a", "flag ", "flag t", "flag F", "flag x", "admin ", "admin s", "admin o", "alias ", "alias b", "run ", "run fo", "run foo b",
            "give x ", "give x y ", "tp ", "tp 1 ", "say hi", "say ", "nonexistent", "nonexistent ", "as 3 ", "as 3 f", "each f", "each foo ", "multi ", "multi a", "opt ", "opt x", "foo \uD83D\uDE00 ", "fOo", "FOO b",
            "pick ", "pick a", "pick x", "pick 1", "pick \u00DF", "pick2 ", "pick2 a", "pick2 al", "pick2 A", "pick2 g", "pick2 b", "m", "w", "wh", "who", "whoami ", "whoami x", "foo bar ", "foo bar x", "big ", "big 5 "};
        for (int src : new int[] {0, 3}) {
            for (String in : suggInputs) {
                ParseResults<Integer> pr = d.parse(in, src);
                line("SUGG " + src + "\t" + in + "\t" + sugg(d.getCompletionSuggestions(pr).join(), in));
            }
        }
        int[][] cursors = {{4, 0}, {5, 3}, {6, 3}, {3, 3}, {0, 3}, {7, 3}};
        String[] cursorInputs = {"foo bar", "foo bar", "foo 12 ab", "foo bar", "foo bar", "foo 12 ab"};
        for (int i = 0; i < cursors.length; i++) {
            ParseResults<Integer> pr = d.parse(cursorInputs[i], cursors[i][1]);
            line("SUGG_AT " + cursors[i][1] + "\t" + cursorInputs[i] + "\t" + cursors[i][0] + "\t" + sugg(d.getCompletionSuggestions(pr, cursors[i][0]).join(), cursorInputs[i].substring(0, cursors[i][0])));
        }

        // ---- parse results shape
        for (String in : new String[] {"foo 5 abc", "foo bar", "foo", "alias bar", "run foo bar", "as 5 whoami", "foo 5 abc extra", "unknown", "tp 1 2", "give a \"b c\" 3"}) {
            ParseResults<Integer> pr = d.parse(in, 0);
            StringBuilder b = new StringBuilder("PARSE " + in + "\t" + "cursor=" + pr.getReader().getCursor() + " range=" + pr.getContext().getRange() + " exc=" + pr.getExceptions().size());
            for (var e : pr.getExceptions().entrySet()) b.append(" {").append(e.getKey()).append(": ").append(e.getValue().getMessage()).append('}');
            for (CommandContextBuilder<Integer> c = pr.getContext(); c != null; c = c.getChild()) {
                b.append(" ctx[");
                for (ParsedCommandNode<Integer> n : c.getNodes()) b.append(n).append(',');
                b.append("cmd=").append(c.getCommand() != null).append(",args=").append(new TreeMap<>(c.getArguments()).keySet());
                for (var a : new TreeMap<>(c.getArguments()).entrySet()) b.append(' ').append(a.getKey()).append('=').append(a.getValue().getResult()).append('@').append(a.getValue().getRange());
                b.append(']');
            }
            line(b.toString());
        }

        // ---- ambiguities
        CommandDispatcher<Integer> amb = new CommandDispatcher<>();
        amb.register(literal("amb").then(argument("a", integer())).then(argument("b", integer(0, 5))).then(argument("w", word())).then(literal("123")).then(literal("x")));
        amb.register(literal("ok").then(literal("a")).then(literal("b")).then(argument("n", integer())));
        List<String> ambOut = new ArrayList<>();
        amb.findAmbiguities((parent, child, sibling, matches) -> ambOut.add(String.join(" ", amb.getPath(parent)) + " | " + child.getName() + " | " + sibling.getName() + " | " + new TreeSet<>(matches)));
        for (String s : ambOut) line("AMBIG " + s);

        // ---- string reader
        String[] readInputs = {"", "0", "-5", "12abc", "1.5", ".5", "5.", "-", "--1", "1-2", "2147483647", "2147483648", "-2147483648", "9223372036854775807", "9223372036854775808", "1e5", "abc", "true", "false", "True", "\"hi\"",
            "'hi'", "\"a\\\"b\"", "\"a\\\\b\"", "\"a\\nb\"", "\"unclosed", "'it''s'", "hi there", "h\u00E9llo", "a+b-c_d.e", "\uD83D\uDE00x", "\"\uD83D\uDE00\"", "\"\uD83D\uDE00\\q\"", "0.1", "340282366920938463463374607431768211456",
            "0.000000000000000000000000000000000000000000001", "123456789.123456789", "1000000", "10000000", "0.001", "0.0001", "-0", "00012", "+5", "\"\"", "\"a\\", "h\u00E9llo \"x\"", "\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9\u00E9 12"};
        for (String in : readInputs) {
            for (String m : new String[] {"int", "long", "double", "float", "unquoted", "quoted", "string", "bool"}) {
                StringReader r = new StringReader(in);
                String res;
                try {
                    res = "OK " + switch (m) {
                        case "int" -> String.valueOf(r.readInt());
                        case "long" -> String.valueOf(r.readLong());
                        case "double" -> String.valueOf(r.readDouble());
                        case "float" -> String.valueOf(r.readFloat());
                        case "unquoted" -> "<" + r.readUnquotedString() + ">";
                        case "quoted" -> "<" + r.readQuotedString() + ">";
                        case "string" -> "<" + r.readString() + ">";
                        default -> String.valueOf(r.readBoolean());
                    };
                } catch (CommandSyntaxException e) { res = "ERR " + e.getMessage(); }
                line("READ " + m + "\t" + in + "\t" + res + "\tcursor=" + r.getCursor() + " read=<" + r.getRead() + "> rem=<" + r.getRemaining() + ">");
            }
        }
        for (String in : new String[] {"", " \t\u00A0x", "\u2003\u0085 y", "  ", "\u001Cx", "\u180Ex", "\u3000z"}) {
            StringReader r = new StringReader(in);
            r.skipWhitespace();
            line("SKIPWS\t" + in + "\t" + r.getCursor());
        }
        for (String in : new String[] {"x", "y", "", "xx"}) {
            StringReader r = new StringReader(in);
            String res;
            try { r.expect('x'); res = "OK"; } catch (CommandSyntaxException e) { res = "ERR " + e.getMessage(); }
            line("EXPECT\t" + in + "\t" + res + "\t" + r.getCursor());
        }

        // ---- argument type bounds, toString, escape
        line("ARG " + norm("" + integer() + " | " + integer(3) + " | " + integer(1, 5) + " | " + longArg() + " | " + longArg(3) + " | " + longArg(1, 5) + " | " + floatArg() + " | " + floatArg(1.5f) + " | " + floatArg(1f, 2.5f) + " | "
            + doubleArg() + " | " + doubleArg(-1) + " | " + doubleArg(1e10, 2.5e-5) + " | " + word() + " | " + string() + " | " + greedyString() + " | " + bool()));
        line("ARG_EQ " + integer(1, 2).equals(integer(1, 2)) + " " + integer(1, 2).equals(integer(1, 3)) + " " + doubleArg(1).equals(doubleArg(1)) + " " + word().equals(word()));
        for (String s : new String[] {"abc", "a b", "", "a\"b", "a\\b", "h\u00E9", "x_y-z.w+1"}) line("ESCAPE\t" + s + "\t" + escapeIfRequired(s));
        for (String s : new String[] {"1.0E10", "x"}) line("EXAMPLES " + word().getExamples() + string().getExamples() + greedyString().getExamples() + integer().getExamples() + doubleArg().getExamples() + bool().getExamples());
        for (String[] t : new String[][] {{"double", "-5", "-1", "1"}}) {}
        // bound errors via reader-based parse
        String[] boundIn = {"-5", "50", "5", "0", "100", "101"};
        for (String bi : boundIn) {
            for (ArgumentType<?> t : new ArgumentType<?>[] {integer(0, 100), longArg(1, 50), floatArg(0.5f, 50.5f), doubleArg(1, 1e10), doubleArg(-1.5e-5, 3), floatArg(1e-5f, 1e8f), doubleArg(1e7), floatArg(-1e7f)}) {
                StringReader r = new StringReader(bi + " tail");
                String res;
                try { res = "OK " + t.parse(r); } catch (CommandSyntaxException e) { res = "ERR " + e.getMessage(); }
                line("BOUND " + t + "\t" + bi + "\t" + res + "\t" + r.getCursor());
            }
        }

        // ---- Java double/float toString
        Random rnd = new Random(12345);
        List<Long> bits = new ArrayList<>(List.of(0L, 0x8000000000000000L, 1L, 0x7FEFFFFFFFFFFFFFL, 0x7FF0000000000000L, 0xFFF0000000000000L, 0x7FF8000000000000L));
        for (double v : new double[] {1.0, 0.5, 1e-3, 9.999e-4, 1e7, 9999999.0, 12345678.9, 1.0E23, 1e22, 123456.789, 0.1 + 0.2, 9007199254740992.0, 1 / 3.0, 100.0, 1e21, -2.5, 1e-5, 2e-323, 3e-323, 1.5e300, 5e-324, 4.9e-324, 2.2250738585072014E-308})
            bits.add(Double.doubleToRawLongBits(v));
        for (int i = 0; i < 1500; i++) bits.add(rnd.nextLong());
        for (int i = 0; i < 500; i++) bits.add(Double.doubleToRawLongBits(rnd.nextInt(100000) / 1000.0));
        for (int i = 0; i < 300; i++) bits.add(Double.doubleToRawLongBits(Math.pow(10, rnd.nextInt(40) - 20)));
        for (long b : bits) line("DBL " + Long.toHexString(b) + " " + Double.toString(Double.longBitsToDouble(b)));
        List<Integer> fbits = new ArrayList<>(List.of(0, 0x80000000, 1, 0x7F7FFFFF, 0x7F800000, 0xFF800000, 0x7FC00000));
        for (float v : new float[] {1.0f, 0.5f, 1e-3f, 9.999e-4f, 1e7f, 9999999.0f, 1.0E10f, 16777216f, 0.1f, 0.3f, 3.4028235E38f, 1.4E-45f, 2.8E-45f, 1e-5f, 123456.79f, 1e22f, 1e23f})
            fbits.add(Float.floatToRawIntBits(v));
        for (int i = 0; i < 1500; i++) fbits.add(rnd.nextInt());
        for (int i = 0; i < 500; i++) fbits.add(Float.floatToRawIntBits(rnd.nextInt(100000) / 100.0f));
        for (int b : fbits) line("FLT " + Integer.toHexString(b) + " " + Float.toString(Float.intBitsToFloat(b)));

        // ---- exception context formatting
        for (int cur : new int[] {0, 1, 9, 10, 11, 12, 30, 32, 33, 100}) {
            String input = "abcdefghijklmnopqrstuvwxyz0123456789".substring(0, 30) + "\uD83D\uDE00\u00E9";
            CommandSyntaxException e = new SimpleCommandExceptionType(new LiteralMessage("m")).createWithContext(readerAt(input, Math.min(cur, input.length())));
            line("CTX " + cur + "\t" + e.getMessage());
        }
        CommandSyntaxException noctx = new SimpleCommandExceptionType(new LiteralMessage("plain")).create();
        line("NOCTX " + noctx.getMessage() + " " + noctx.getContext() + " " + noctx.getInput() + " " + noctx.getCursor());
        CommandSyntaxException big = new SimpleCommandExceptionType(new LiteralMessage("big")).createWithContext(readerAt("short", 5));
        line("CTX_SHORT " + big.getMessage());
        CommandSyntaxException dyn = new Dynamic2CommandExceptionType((a, b) -> new LiteralMessage(a + "/" + b)).create(1, "x");
        line("DYN2 " + dyn.getMessage());

        Files.writeString(Path.of(args[0]), out.toString(), StandardCharsets.UTF_8);
    }

    static StringReader readerAt(String s, int cursor) { StringReader r = new StringReader(s); r.setCursor(cursor); return r; }
}
