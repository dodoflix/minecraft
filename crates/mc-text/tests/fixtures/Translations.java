import com.google.gson.*;
import net.minecraft.network.chat.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for translatable formatting: for every key of en_us.json (plus keys from deprecated.json and a few
 * unknown ones) the plain text of `Component.translatable(key, a1..an).getString()` for n in {0, 2, 6}.
 * Usage: scripts/vanilla-java.sh -o crates/mc-text/tests/fixtures/translations.jsonl crates/mc-text/tests/fixtures/Translations.java ref/src/assets/minecraft/lang
 * Output: crates/mc-text/tests/fixtures/translations.jsonl
 */
public class Translations {
    public static void main(String[] args) throws Exception {
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");
        Gson gson = new GsonBuilder().disableHtmlEscaping().create();
        JsonObject en = JsonParser.parseString(Files.readString(Path.of(args[0], "en_us.json"))).getAsJsonObject();
        JsonObject dep = JsonParser.parseString(Files.readString(Path.of(args[0], "deprecated.json"))).getAsJsonObject();
        List<String> keys = new ArrayList<>(en.keySet());
        dep.getAsJsonArray("removed").forEach(e -> keys.add(e.getAsString()));
        dep.getAsJsonObject("renamed").entrySet().forEach(e -> { keys.add(e.getKey()); keys.add(e.getValue().getAsString()); });
        keys.addAll(List.of("unknown.key", "", "%s", "a.b.c"));
        int[] counts = {0, 2, 6};
        for (String key : keys) {
            JsonArray o = new JsonArray();
            for (int n : counts) {
                Object[] a = new Object[n];
                for (int i = 0; i < n; i++) a[i] = "<a" + (i + 1) + ">";
                o.add(Component.translatable(key, a).getString());
            }
            JsonObject rec = new JsonObject();
            rec.addProperty("k", key);
            rec.add("o", o);
            out.println(gson.toJson(rec));
        }
    }
}
