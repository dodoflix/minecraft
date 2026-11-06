import com.google.gson.*;
import com.mojang.serialization.*;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for TextComponentTagVisitor (nbt_text.rs): for every `T<snbt>` line of corpus_nbt.txt and the
 * compound/list `N<snbt>` lines of corpus.txt, the component (as JSON) for indentation {"", "  "} x styling {plain, rich},
 * with sorted keys (vanilla default depends on the logger level).
 * Usage: scripts/vanilla-java.sh -o crates/mc-text/tests/fixtures/nbt_text.jsonl crates/mc-text/tests/fixtures/NbtText.java crates/mc-text/tests/fixtures/corpus_nbt.txt crates/mc-text/tests/fixtures/corpus.txt
 * Output: crates/mc-text/tests/fixtures/nbt_text.jsonl
 */
public class NbtText {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");
        Gson gson = new GsonBuilder().disableHtmlEscaping().create();
        int n = 0;
        for (String file : args) {
            for (String line : Files.readAllLines(Path.of(file))) {
                if (line.length() < 2 || (line.charAt(0) != 'T' && !(line.charAt(0) == 'N' && n++ < 120))) continue;
                Tag tag = TagParser.create(NbtOps.INSTANCE).parseFully(line.substring(1));
                ByteArrayOutputStream bos = new ByteArrayOutputStream();
                NbtIo.writeUnnamedTag(tag, new DataOutputStream(bos));
                StringBuilder hex = new StringBuilder();
                for (byte b : bos.toByteArray()) hex.append(String.format(java.util.Locale.ROOT, "%02x", b));
                JsonObject rec = new JsonObject();
                rec.addProperty("src", line);
                rec.addProperty("nbt", hex.toString());
                JsonArray outs = new JsonArray();
                for (String indent : new String[]{"", "  "}) {
                    for (boolean rich : new boolean[]{false, true}) {
                        TextComponentTagVisitor v = new TextComponentTagVisitor(indent,
                            rich ? TextComponentTagVisitor.RichStyling.INSTANCE : TextComponentTagVisitor.PlainStyling.INSTANCE, true);
                        Component c = v.visit(tag);
                        outs.add(ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, c).getOrThrow());
                    }
                }
                rec.add("out", outs);
                out.println(gson.toJson(rec));
            }
        }
    }
}
