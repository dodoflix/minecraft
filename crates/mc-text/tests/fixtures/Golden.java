import com.google.gson.*;
import com.mojang.serialization.*;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for mc-text (stdout is captured into the fixture by the `-o` wrapper).
 * Usage: scripts/vanilla-java.sh -o crates/mc-text/tests/fixtures/golden.jsonl.gz crates/mc-text/tests/fixtures/Golden.java crates/mc-text/tests/fixtures/corpus.txt crates/mc-text/tests/fixtures/corpus_random.txt
 * Output: crates/mc-text/tests/fixtures/golden.jsonl.gz
 *
 * Corpus lines: `J<json>` decodes with JsonOps, `N<snbt>` decodes with NbtOps. Blank lines and `#` comments are skipped.
 * One JSON object per output line:
 *   src, kind, input (JSON text or hex of the unnamed NBT), ok, and on success: json (re-encoded, JSON text),
 *   nbt (re-encoded, hex of the unnamed NBT), string (getString()); on failure: err.
 */
public class Golden {
    static String hex(Tag t) throws IOException {
        ByteArrayOutputStream bos = new ByteArrayOutputStream();
        NbtIo.writeUnnamedTag(t, new DataOutputStream(bos));
        StringBuilder sb = new StringBuilder();
        for (byte b : bos.toByteArray()) sb.append(String.format(java.util.Locale.ROOT, "%02x", b));
        return sb.toString();
    }

    static List<String> allLines(String[] files) throws IOException {
        List<String> all = new ArrayList<>();
        for (String f : files) all.addAll(Files.readAllLines(Path.of(f)));
        return all;
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");
        Gson gson = new GsonBuilder().disableHtmlEscaping().create();
        for (String line : allLines(args)) {
            if (line.isBlank() || line.startsWith("#")) continue;
            char kind = line.charAt(0);
            String body = line.substring(1);
            JsonObject rec = new JsonObject();
            rec.addProperty("src", line);
            rec.addProperty("kind", String.valueOf(kind));
            DataResult<Component> r;
            JsonArray restricted = new JsonArray(); // accepted by flatRestrictedCodec(max) for max in {5, 20, 100, 1000}
            JsonElement el = null;
            Tag t = null;
            try {
                if (kind == 'J') {
                    rec.addProperty("input", body);
                    el = JsonParser.parseString(body);
                } else {
                    t = TagParser.create(NbtOps.INSTANCE).parseFully(body);
                    rec.addProperty("input", hex(t));
                }
            } catch (Exception e) {
                System.err.println("corpus line does not parse: " + line + " -> " + e);
                System.exit(1);
                return;
            }
            try {
                if (kind == 'J') {
                    r = ComponentSerialization.CODEC.parse(JsonOps.INSTANCE, el);
                    for (int max : new int[]{5, 20, 100, 1000}) restricted.add(ComponentSerialization.flatRestrictedCodec(max).parse(JsonOps.INSTANCE, el).isSuccess());
                } else {
                    r = ComponentSerialization.CODEC.parse(NbtOps.INSTANCE, t);
                    for (int max : new int[]{5, 20, 100, 1000}) restricted.add(ComponentSerialization.flatRestrictedCodec(max).parse(NbtOps.INSTANCE, t).isSuccess());
                }
            } catch (Throwable e) {
                // vanilla throws (a server would fail the packet / command): record as a rejection
                r = DataResult.error(() -> "EXCEPTION " + e);
                rec.addProperty("exception", true);
            }
            if (r.isError()) {
                rec.addProperty("ok", false);
                rec.addProperty("err", r.error().get().message());
            } else {
                Component c = r.getOrThrow();
                DataResult<JsonElement> j = ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, c);
                DataResult<Tag> n = ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, c);
                rec.addProperty("ok", true);
                rec.add("restricted", restricted);
                if (j.isError()) rec.addProperty("json_err", j.error().get().message()); else rec.addProperty("json", gson.toJson(j.getOrThrow()));
                if (n.isError()) rec.addProperty("nbt_err", n.error().get().message()); else rec.addProperty("nbt", hex(n.getOrThrow()));
                rec.addProperty("string", c.getString());
                // getString(limit) for several limits; null where the cut would split a surrogate pair
                JsonArray limits = new JsonArray();
                for (int lim : new int[]{0, 1, 2, 3, 5, 100}) {
                    String ls = c.getString(lim);
                    if (!ls.isEmpty() && Character.isHighSurrogate(ls.charAt(ls.length() - 1))) limits.add(JsonNull.INSTANCE); else limits.add(ls);
                }
                rec.add("limits", limits);
                // toFlatList(): every non-empty styled piece as an encoded component
                // (the string "ERR" when some piece cannot be encoded, e.g. sprite fonts)
                JsonArray flat = new JsonArray();
                boolean flatErr = false;
                for (Component f : c.toFlatList()) {
                    DataResult<JsonElement> fr = ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, f);
                    if (fr.isError()) flatErr = true; else flat.add(fr.getOrThrow());
                }
                rec.add("flat", flatErr ? new JsonPrimitive("ERR") : flat);
                // contains(): itself, the first sibling, a literal
                JsonArray contains = new JsonArray();
                contains.add(c.contains(c));
                contains.add(c.getSiblings().isEmpty() ? JsonNull.INSTANCE : new JsonPrimitive(c.contains(c.getSiblings().get(0))));
                contains.add(c.contains(Component.literal("a")));
                rec.add("contains", contains);
            }
            out.println(gson.toJson(rec));
        }
    }
}
