import com.google.gson.*;
import com.mojang.serialization.*;
import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.*;
import net.minecraft.util.StringDecomposer;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for TextColor / ChatFormatting / StringDecomposer / Style operations.
 * Usage: scripts/vanilla-java.sh -o crates/mc-text/tests/fixtures/misc.jsonl crates/mc-text/tests/fixtures/Misc.java crates/mc-text/tests/fixtures/golden.jsonl.gz
 * Output: crates/mc-text/tests/fixtures/misc.jsonl
 * (styles are taken from the J cases of golden.jsonl that decode successfully)
 */
public class Misc {
    static final Gson GSON = new GsonBuilder().disableHtmlEscaping().create();

    static JsonElement style(Style s) {
        return Style.Serializer.CODEC.encodeStart(JsonOps.INSTANCE, s).getOrThrow();
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
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");

        for (String in : new String[]{"a", "§aGreen§r plain", "§", "x§", "§Zno", "§K§L§O§R§F upper", "§§a", "§0§1§2§3§4§5§6§7§8§9§a§b§c§d§e§f§k§l§m§n§o§r", "§g§p§q", "😀§a😀", ""}) {
            JsonObject r = new JsonObject();
            r.addProperty("t", "strip");
            r.addProperty("in", in);
            r.addProperty("out", ChatFormatting.stripFormatting(in));
            r.addProperty("plain", StringDecomposer.getPlainText(Component.literal(in)));
            out.println(GSON.toJson(r));
        }
        for (char c : "0123456789abcdefklmnorABCDEFKLMNORxzÀ§ ".toCharArray()) {
            JsonObject r = new JsonObject();
            r.addProperty("t", "code");
            r.addProperty("in", String.valueOf(c));
            ChatFormatting f = ChatFormatting.getByCode(c);
            r.addProperty("out", f == null ? null : f.name());
            out.println(GSON.toJson(r));
        }
        for (String in : new String[]{"red", "RED", "#FF0000", "#ff0000", "#fff", "#", "#-0", "#+1", "#+", "#1000000", "#FFFFFFFF", "#80000000", "#-1", "#12345G", "dark_blue", "dark blue", "", "#0x10", "# 1", "#00_1", "white", "#7fffffff", "#ffffff "}) {
            JsonObject r = new JsonObject();
            r.addProperty("t", "color");
            r.addProperty("in", in);
            DataResult<TextColor> res = TextColor.parseColor(in);
            r.addProperty("ok", res.isSuccess());
            if (res.isSuccess()) {
                r.addProperty("ser", res.getOrThrow().serialize());
                r.addProperty("value", res.getOrThrow().getValue());
            }
            out.println(GSON.toJson(r));
        }

        List<Style> styles = new ArrayList<>();
        for (String line : gzLines(args[0])) {
            JsonObject rec = JsonParser.parseString(line).getAsJsonObject();
            if (!rec.get("kind").getAsString().equals("J") || !rec.get("ok").getAsBoolean()) continue;
            Component c = ComponentSerialization.CODEC.parse(JsonOps.INSTANCE, JsonParser.parseString(rec.get("input").getAsString())).getOrThrow();
            styles.add(c.getStyle());
            for (Component s : c.getSiblings()) styles.add(s.getStyle());
        }
        for (int i = 0; i < styles.size(); i += 4) {
            Style a = styles.get(i), b = styles.get((i * 7 + 3) % styles.size());
            JsonObject r = new JsonObject();
            r.addProperty("t", "style");
            r.add("a", style(a));
            r.add("b", style(b));
            r.add("apply_to", style(a.applyTo(b)));
            r.addProperty("empty", a.isEmpty());
            JsonObject fmt = new JsonObject(), legacy = new JsonObject();
            for (ChatFormatting f : ChatFormatting.values()) {
                fmt.add(f.name(), style(a.applyFormat(f)));
                legacy.add(f.name(), style(a.applyLegacyFormat(f)));
            }
            r.add("apply_format", fmt);
            r.add("apply_legacy_format", legacy);
            r.add("apply_formats_1", style(a.applyFormats(ChatFormatting.RED, ChatFormatting.BOLD)));
            r.add("apply_formats_2", style(a.applyFormats(ChatFormatting.BOLD, ChatFormatting.RESET, ChatFormatting.ITALIC)));
            r.add("without_shadow", style(a.withoutShadow()));
            r.add("with_bold_null", style(a.withBold(null)));
            r.add("with_color_null", style(a.withColor((TextColor) null)));
            r.add("with_color_0x123456", style(a.withColor(0x123456)));
            r.add("with_color_gold", style(a.withColor(ChatFormatting.GOLD)));
            r.add("with_insertion_x", style(a.withInsertion("x")));
            r.add("with_click_null", style(a.withClickEvent(null)));
            out.println(GSON.toJson(r));
        }
    }
}
