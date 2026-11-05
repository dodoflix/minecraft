// Golden generator for the dialog codecs (crates/mc-protocol/tests/dialog_golden.rs): vanilla's Dialog.CODEC / STREAM_CODEC /
// CONTEXT_FREE_STREAM_CODEC over a RegistryOps of the booted vanilla registries (the data pack's three dialogs exist as
// references).   crates/mc-protocol/tests/fixtures/dialog/gen.sh
//
// Corpus lines (corpus.txt, blank lines and `#` comments skipped):
//   J<json>   Dialog.CODEC.parse(RegistryOps<JsonElement>, json)
//   N<snbt>   Dialog.CODEC.parse(RegistryOps<Tag>, tag)
//   S<snbt>   stream bytes `00` (inline holder) + network NBT of the snbt, decoded with Dialog.STREAM_CODEC
//   X<hex>    raw stream bytes decoded with Dialog.STREAM_CODEC
//   C<snbt>   network NBT of the snbt, decoded with Dialog.CONTEXT_FREE_STREAM_CODEC
// One JSON object per output line: src, kind, input (json text / hex), ok, err; on success
//   holder ("reference"|"direct"), json (re-encoded), nbt (re-encoded, hex of the unnamed NBT), stream (STREAM_CODEC bytes),
//   cf (CONTEXT_FREE_STREAM_CODEC bytes; direct only).
// Stream cases: `re` = bytes encoded back from the decoded value.
import com.google.gson.*;
import com.mojang.serialization.*;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.dialog.Dialog;

public class DialogGolden {
    static String hex(Tag t) throws IOException {
        ByteArrayOutputStream bos = new ByteArrayOutputStream();
        NbtIo.writeUnnamedTag(t, new DataOutputStream(bos));
        return HexFormat.of().formatHex(bos.toByteArray());
    }

    static byte[] unhex(String s) { return HexFormat.of().parseHex(s); }

    public static void main(String[] args) throws Exception {
        RegistryAccess.Frozen access = extractor.Setup.bootstrap();
        RegistryOps<JsonElement> jops = access.createSerializationContext(JsonOps.INSTANCE);
        RegistryOps<Tag> nops = access.createSerializationContext(NbtOps.INSTANCE);
        PrintStream out = new PrintStream(new FileOutputStream(args[1]), true, "UTF-8");
        Gson gson = new GsonBuilder().disableHtmlEscaping().create();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isBlank() || line.startsWith("#")) continue;
            char kind = line.charAt(0);
            String body = line.substring(1);
            JsonObject rec = new JsonObject();
            rec.addProperty("src", line);
            rec.addProperty("kind", String.valueOf(kind));
            try {
                switch (kind) {
                    case 'J', 'N' -> {
                        DataResult<Holder<Dialog>> r;
                        if (kind == 'J') {
                            rec.addProperty("input", body);
                            r = Dialog.CODEC.parse(jops, JsonParser.parseString(body));
                        } else {
                            Tag t = TagParser.create(NbtOps.INSTANCE).parseFully(body);
                            rec.addProperty("input", hex(t));
                            r = Dialog.CODEC.parse(nops, t);
                        }
                        if (r.isError()) {
                            rec.addProperty("ok", false);
                            rec.addProperty("err", r.error().get().message());
                        } else {
                            Holder<Dialog> h = r.getOrThrow();
                            rec.addProperty("ok", true);
                            rec.addProperty("holder", h.kind() == Holder.Kind.REFERENCE ? "reference" : "direct");
                            rec.addProperty("json", gson.toJson(Dialog.CODEC.encodeStart(jops, h).getOrThrow()));
                            rec.addProperty("nbt", hex(Dialog.CODEC.encodeStart(nops, h).getOrThrow()));
                            RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
                            Dialog.STREAM_CODEC.encode(buf, h);
                            rec.addProperty("stream", ByteBufUtil.hexDump(buf));
                            if (h.kind() == Holder.Kind.DIRECT) {
                                ByteBuf cf = Unpooled.buffer();
                                Dialog.CONTEXT_FREE_STREAM_CODEC.encode(cf, h.value());
                                rec.addProperty("cf", ByteBufUtil.hexDump(cf));
                            }
                        }
                    }
                    case 'S', 'X', 'C' -> {
                        byte[] bytes;
                        if (kind == 'X') {
                            bytes = unhex(body);
                        } else {
                            Tag t = TagParser.create(NbtOps.INSTANCE).parseFully(body);
                            ByteArrayOutputStream bos = new ByteArrayOutputStream();
                            if (kind == 'S') bos.write(0);
                            NbtIo.writeUnnamedTag(t, new DataOutputStream(bos));
                            bytes = bos.toByteArray();
                        }
                        rec.addProperty("input", HexFormat.of().formatHex(bytes));
                        try {
                            if (kind == 'C') {
                                ByteBuf in = Unpooled.wrappedBuffer(bytes);
                                Dialog d = Dialog.CONTEXT_FREE_STREAM_CODEC.decode(in);
                                if (in.isReadable()) throw new IllegalStateException(in.readableBytes() + " trailing bytes");
                                ByteBuf re = Unpooled.buffer();
                                Dialog.CONTEXT_FREE_STREAM_CODEC.encode(re, d);
                                rec.addProperty("ok", true);
                                rec.addProperty("re", ByteBufUtil.hexDump(re));
                            } else {
                                RegistryFriendlyByteBuf in = new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(bytes), access);
                                Holder<Dialog> h = Dialog.STREAM_CODEC.decode(in);
                                if (in.isReadable()) throw new IllegalStateException(in.readableBytes() + " trailing bytes");
                                RegistryFriendlyByteBuf re = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
                                Dialog.STREAM_CODEC.encode(re, h);
                                rec.addProperty("ok", true);
                                rec.addProperty("re", ByteBufUtil.hexDump(re));
                            }
                        } catch (Throwable e) {
                            rec.addProperty("ok", false);
                            rec.addProperty("err", String.valueOf(e.getMessage()));
                        }
                    }
                    default -> throw new IllegalStateException("bad corpus line kind: " + line);
                }
            } catch (Throwable e) {
                // vanilla throws outside the DataResult (a server would fail the packet / command): record as a rejection
                rec.addProperty("ok", false);
                rec.addProperty("exception", true);
                rec.addProperty("err", String.valueOf(e));
            }
            out.println(gson.toJson(rec));
        }
        out.close();
    }
}
