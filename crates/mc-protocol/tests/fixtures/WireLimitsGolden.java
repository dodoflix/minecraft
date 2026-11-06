// Generates tests/fixtures/wire_limits.golden: what vanilla's encoders do with values at/over their write-side limits.
// Line format (tab separated): `label<TAB>OK<TAB>hex` or `label<TAB>ERR<TAB>Throwable.toString()[<TAB>cause=Throwable.toString()][<TAB>skippable]`.
// Usage: crates/mc-protocol/tests/fixtures/wire_limits.sh   (compiles tools/extractor Setup + play/PlayHarness, then runs this)
// Output: crates/mc-protocol/tests/fixtures/wire_limits.golden.gz
import com.google.common.collect.ImmutableMultimap;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import java.io.PrintWriter;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.nbt.*;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.chat.LastSeenMessages;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.ClientboundCustomReportDetailsPacket;
import net.minecraft.network.protocol.common.ClientboundStoreCookiePacket;
import net.minecraft.network.protocol.configuration.ConfigurationProtocols;
import net.minecraft.network.protocol.configuration.ServerboundSelectKnownPacks;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.network.protocol.game.ClientboundTagQueryPacket;
import net.minecraft.network.protocol.game.ServerboundChatPacket;
import net.minecraft.network.protocol.game.ServerboundEditBookPacket;
import net.minecraft.network.protocol.game.ServerboundSignUpdatePacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.level.ParticleStatus;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.network.protocol.common.ServerboundClientInformationPacket;

public class WireLimitsGolden {
    static PrintWriter W;

    static String rep(String s, int n) { return s.repeat(n); }

    static String thr(Throwable t) { return t.toString(); }

    /** Run `encode` into a fresh buffer; print OK+hex or ERR+exception text. */
    static void run(String label, java.util.function.Consumer<ByteBuf> encode) {
        ByteBuf buf = Unpooled.buffer();
        try {
            encode.accept(buf);
            W.println(label + "\tOK\t" + ByteBufUtil.hexDump(buf));
        } catch (Throwable t) {
            StringBuilder sb = new StringBuilder(label + "\tERR\t" + thr(t));
            if (t.getCause() != null) sb.append("\tcause=").append(thr(t.getCause()));
            W.println(sb);
        }
    }

    static <T> void codec(String label, StreamCodec<? super ByteBuf, T> c, T v) {
        run(label, b -> c.encode(b, v));
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    static void packet(String label, ProtocolInfo<?> info, Packet<?> p) {
        ByteBuf buf = Unpooled.buffer();
        try {
            ((StreamCodec) info.codec()).encode(buf, p);
            W.println(label + "\tOK\t" + ByteBufUtil.hexDump(buf));
        } catch (Throwable t) {
            StringBuilder sb = new StringBuilder(label + "\tERR\t" + thr(t));
            if (t.getCause() != null) sb.append("\tcause=").append(thr(t.getCause()));
            if (p.isSkippable()) sb.append("\tskippable");
            W.println(sb);
        }
    }

    static Identifier id(String s) { return Identifier.parse(s); }

    public static void main(String[] args) throws Exception {
        PlayHarness.init("/dev/null");
        W = new PrintWriter(args[0]);

        // ---- strings: stringUtf8(max) counts UTF-16 units; the byte bound is 3 * max
        String smile = new String(Character.toChars(0x1F600)); // 2 UTF-16 units, 4 UTF-8 bytes
        int[] maxes = {0, 1, 4, 16, 20, 32, 40, 64, 128, 256, 1024, 4096, 32500, 32767};
        for (int max : maxes) {
            StreamCodec<ByteBuf, String> c = ByteBufCodecs.stringUtf8(max);
            codec("str/" + max + "/at_limit", c, rep("a", max));
            codec("str/" + max + "/over", c, rep("a", max + 1));
            codec("str/" + max + "/over_by_many", c, rep("a", max + 40000));
        }
        codec("str/4/smile2_ok", ByteBufCodecs.stringUtf8(4), smile + smile);
        codec("str/4/smile2_a_over", ByteBufCodecs.stringUtf8(4), smile + smile + "a");
        codec("str/3/smile2_over", ByteBufCodecs.stringUtf8(3), smile + smile);
        codec("str/16/e_acute_16", ByteBufCodecs.stringUtf8(16), rep("\u00e9", 16));
        codec("str/16/e_acute_17", ByteBufCodecs.stringUtf8(16), rep("\u00e9", 17));
        codec("str/32767/euro_max", ByteBufCodecs.stringUtf8(32767), rep("\u20ac", 32767));
        codec("str/32767/euro_over", ByteBufCodecs.stringUtf8(32767), rep("\u20ac", 32768));
        codec("str/32767/smile_max_units", ByteBufCodecs.stringUtf8(32767), rep(smile, 16383) + "a");
        codec("str/32767/smile_over_units", ByteBufCodecs.stringUtf8(32767), rep(smile, 16384));
        codec("str/default/empty", ByteBufCodecs.STRING_UTF8, "");
        run("friendly/writeUtf_default_over", b -> new FriendlyByteBuf(b).writeUtf(rep("a", 32768)));
        run("friendly/writeUtf_16_over", b -> new FriendlyByteBuf(b).writeUtf(rep("a", 17), 16));
        run("friendly/writeUtf_16_ok", b -> new FriendlyByteBuf(b).writeUtf(rep("a", 16), 16));

        // ---- byte arrays
        for (int max : new int[]{0, 1, 512, 2048, 5120, 2097152}) {
            StreamCodec<ByteBuf, byte[]> c = ByteBufCodecs.byteArray(max);
            codec("bytes/" + max + "/at_limit", c, new byte[max]);
            codec("bytes/" + max + "/over", c, new byte[max + 1]);
        }
        run("friendly/writeByteArray_unlimited", b -> FriendlyByteBuf.writeByteArray(b, new byte[100000]));

        // ---- lists / collections (INT elements)
        for (int max : new int[]{0, 1, 20, 32, 64, 100, 128, 256}) {
            StreamCodec<ByteBuf, List<Integer>> c = ByteBufCodecs.INT.apply(ByteBufCodecs.list(max));
            codec("list/" + max + "/at_limit", c, Collections.nCopies(max, 7));
            codec("list/" + max + "/over", c, Collections.nCopies(max + 1, 7));
        }
        codec("list/unlimited/1000", ByteBufCodecs.INT.apply(ByteBufCodecs.list()), Collections.nCopies(1000, 1));
        StreamCodec<ByteBuf, List<Integer>> fixed4 = ByteBufCodecs.INT.apply(ByteBufCodecs.fixedSizeList(4));
        codec("fixedlist/4/ok", fixed4, List.of(1, 2, 3, 4));
        codec("fixedlist/4/short", fixed4, List.of(1, 2, 3));
        codec("fixedlist/4/long", fixed4, List.of(1, 2, 3, 4, 5));

        // ---- maps
        for (int max : new int[]{1, 32, 128, 256}) {
            StreamCodec<ByteBuf, Map<Integer, Integer>> c = ByteBufCodecs.map(LinkedHashMap::new, ByteBufCodecs.INT, ByteBufCodecs.INT, max);
            Map<Integer, Integer> ok = new LinkedHashMap<>(), over = new LinkedHashMap<>();
            for (int i = 0; i < max; i++) ok.put(i, i * 3);
            over.putAll(ok);
            over.put(max, 0);
            codec("map/" + max + "/at_limit", c, ok);
            codec("map/" + max + "/over", c, over);
        }

        // ---- bit sets
        run("fixedbitset/8/ok", b -> new FriendlyByteBuf(b).writeFixedBitSet(bits(0, 7), 8));
        run("fixedbitset/8/over", b -> new FriendlyByteBuf(b).writeFixedBitSet(bits(0, 8), 8));
        run("fixedbitset/20/over", b -> new FriendlyByteBuf(b).writeFixedBitSet(bits(19, 20), 20));
        run("fixedbitset/20/ok", b -> new FriendlyByteBuf(b).writeFixedBitSet(bits(19), 20));

        // ---- NBT (ByteBufCodecs.TAG: End is rejected; strings are modified UTF-8, 65535 bytes at most; no write-side quota)
        codec("nbt/tag/end", ByteBufCodecs.TAG, EndTag.INSTANCE);
        codec("nbt/tag/empty_compound", ByteBufCodecs.TAG, new CompoundTag());
        codec("nbt/tag/string_65535", ByteBufCodecs.TAG, StringTag.valueOf(rep("a", 65535)));
        codec("nbt/tag/string_65536", ByteBufCodecs.TAG, StringTag.valueOf(rep("a", 65536)));
        codec("nbt/tag/string_65537", ByteBufCodecs.TAG, StringTag.valueOf(rep("a", 65537)));
        codec("nbt/tag/string_e_acute_32767", ByteBufCodecs.TAG, StringTag.valueOf(rep("\u00e9", 32767)));
        codec("nbt/tag/string_e_acute_32768", ByteBufCodecs.TAG, StringTag.valueOf(rep("\u00e9", 32768)));
        codec("nbt/tag/string_euro_21845", ByteBufCodecs.TAG, StringTag.valueOf(rep("\u20ac", 21845)));
        codec("nbt/tag/string_euro_21846", ByteBufCodecs.TAG, StringTag.valueOf(rep("\u20ac", 21846)));
        codec("nbt/tag/string_nul_32767", ByteBufCodecs.TAG, StringTag.valueOf(rep("\0", 32767)));
        codec("nbt/tag/string_nul_32768", ByteBufCodecs.TAG, StringTag.valueOf(rep("\0", 32768)));
        codec("nbt/tag/string_smile_16383", ByteBufCodecs.TAG, StringTag.valueOf(rep(smile, 16383)));
        codec("nbt/tag/string_smile_16384", ByteBufCodecs.TAG, StringTag.valueOf(rep(smile, 16384)));
        CompoundTag longValue = new CompoundTag();
        longValue.putString("k", rep("b", 70000));
        codec("nbt/compound/long_value", ByteBufCodecs.COMPOUND_TAG, longValue);
        CompoundTag longKey = new CompoundTag();
        longKey.putInt(rep("k", 65536), 1);
        codec("nbt/compound/long_key", ByteBufCodecs.COMPOUND_TAG, longKey);
        CompoundTag longKeyOk = new CompoundTag();
        longKeyOk.putInt(rep("k", 65535), 1);
        codec("nbt/compound/key_65535", ByteBufCodecs.COMPOUND_TAG, longKeyOk);
        ListTag lst = new ListTag();
        lst.add(StringTag.valueOf(rep("c", 65536)));
        CompoundTag inList = new CompoundTag();
        inList.put("l", lst);
        codec("nbt/compound/long_string_in_list", ByteBufCodecs.TRUSTED_COMPOUND_TAG, inList);
        // a compound larger than the default read quota (2 MiB) still writes fine: no write-side NbtAccounter
        CompoundTag big = new CompoundTag();
        for (int i = 0; i < 40; i++) big.putString("s" + i, rep("z", 60000));
        run("nbt/compound/over_read_quota", b -> {
            ByteBufCodecs.COMPOUND_TAG.encode(b, big);
            if (b.readableBytes() < 2_000_000) throw new IllegalStateException("fixture too small");
            b.clear();
            b.writeBytes(new byte[]{1}); // keep the hex short: only the verdict matters
        });
        // depth: vanilla writes any depth (the read side enforces 512)
        CompoundTag deep = new CompoundTag();
        CompoundTag cur = deep;
        for (int i = 0; i < 600; i++) {
            CompoundTag next = new CompoundTag();
            cur.put("a", next);
            cur = next;
        }
        run("nbt/compound/depth_600", b -> {
            ByteBufCodecs.COMPOUND_TAG.encode(b, deep);
            int n = b.readableBytes();
            b.clear();
            b.writeInt(n);
        });
        codec("nbt/net/write_null", new StreamCodec<ByteBuf, Tag>() {
            public Tag decode(ByteBuf in) { return null; }
            public void encode(ByteBuf out, Tag t) { FriendlyByteBuf.writeNbt(out, null); }
        }, null);

        // ---- Components (TRUSTED_CONTEXT_FREE_STREAM_CODEC = fromCodecTrusted(ComponentSerialization.CODEC))
        StreamCodec<ByteBuf, Component> cc = ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC;
        codec("component/literal_empty", cc, Component.empty());
        codec("component/literal_hi", cc, Component.literal("hi"));
        codec("component/literal_65535", cc, Component.literal(rep("a", 65535)));
        codec("component/literal_65536", cc, Component.literal(rep("a", 65536)));
        codec("component/translatable_key_65536", cc, Component.translatable(rep("t", 65536)));
        codec("component/translatable_key_ok", cc, Component.translatable("chat.type.text", "a", "b"));
        codec("component/nested_long_string", cc, Component.literal("x").append(Component.literal(rep("a", 70000))));
        codec("component/stream_string_100000", ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC, Component.literal(rep("\u20ac", 30000)));

        // ---- whole packets through the real codecs (IdDispatchCodec wraps encoder failures)
        var cc2 = ConfigurationProtocols.CLIENTBOUND;
        var cs2 = ConfigurationProtocols.SERVERBOUND;
        Map<String, String> details = new LinkedHashMap<>();
        for (int i = 0; i < 32; i++) details.put("k" + i, "v");
        packet("packet/report_details_32", cc2, new ClientboundCustomReportDetailsPacket(details));
        details.put("k32", "v");
        packet("packet/report_details_33", cc2, new ClientboundCustomReportDetailsPacket(details));
        packet("packet/report_details_long_key", cc2, new ClientboundCustomReportDetailsPacket(Map.of(rep("k", 129), "v")));
        packet("packet/report_details_long_value", cc2, new ClientboundCustomReportDetailsPacket(Map.of("k", rep("v", 4097))));
        packet("packet/report_details_max_value", cc2, new ClientboundCustomReportDetailsPacket(Map.of(rep("k", 128), rep("v", 4096))));
        packet("packet/store_cookie_5120", cc2, new ClientboundStoreCookiePacket(id("mod:c"), new byte[5120]));
        packet("packet/store_cookie_5121", cc2, new ClientboundStoreCookiePacket(id("mod:c"), new byte[5121]));
        List<net.minecraft.server.packs.repository.KnownPack> packs = new ArrayList<>();
        for (int i = 0; i < 64; i++) packs.add(new net.minecraft.server.packs.repository.KnownPack("minecraft", "p" + i, "1"));
        packet("packet/known_packs_64", cs2, new ServerboundSelectKnownPacks(packs));
        packs.add(new net.minecraft.server.packs.repository.KnownPack("minecraft", "p64", "1"));
        packet("packet/known_packs_65", cs2, new ServerboundSelectKnownPacks(packs));
        packet("packet/client_information_lang_16", cs2, new ServerboundClientInformationPacket(new ClientInformation(rep("l", 16), 12, ChatVisiblity.FULL, true, 0x7f, HumanoidArm.RIGHT, false, true, ParticleStatus.ALL)));
        packet("packet/client_information_lang_17", cs2, new ServerboundClientInformationPacket(new ClientInformation(rep("l", 17), 12, ChatVisiblity.FULL, true, 0x7f, HumanoidArm.RIGHT, false, true, ParticleStatus.ALL)));

        var pcb = PlayHarness.clientbound;
        var psb = PlayHarness.serverbound;
        var upd = new LastSeenMessages.Update(0, new java.util.BitSet(), (byte) 0);
        packet("packet/chat_256", psb, new ServerboundChatPacket(rep("a", 256), java.time.Instant.EPOCH, 0L, Optional.empty(), upd));
        packet("packet/chat_257", psb, new ServerboundChatPacket(rep("a", 257), java.time.Instant.EPOCH, 0L, Optional.empty(), upd));
        packet("packet/edit_book_100_pages", psb, new ServerboundEditBookPacket(0, Collections.nCopies(100, "p"), Optional.empty()));
        packet("packet/edit_book_101_pages", psb, new ServerboundEditBookPacket(0, Collections.nCopies(101, "p"), Optional.empty()));
        packet("packet/edit_book_page_1024", psb, new ServerboundEditBookPacket(0, List.of(rep("p", 1024)), Optional.empty()));
        packet("packet/edit_book_page_1025", psb, new ServerboundEditBookPacket(0, List.of(rep("p", 1025)), Optional.empty()));
        packet("packet/edit_book_title_32", psb, new ServerboundEditBookPacket(0, List.of(), Optional.of(rep("t", 32))));
        packet("packet/edit_book_title_33", psb, new ServerboundEditBookPacket(0, List.of(), Optional.of(rep("t", 33))));
        packet("packet/sign_384", psb, new ServerboundSignUpdatePacket(new net.minecraft.core.BlockPos(1, 2, 3), List.of(rep("s", 384), "", "", ""), net.minecraft.world.level.block.entity.SignTextSlot.FRONT));
        packet("packet/sign_385", psb, new ServerboundSignUpdatePacket(new net.minecraft.core.BlockPos(1, 2, 3), List.of("", rep("s", 385), "", ""), net.minecraft.world.level.block.entity.SignTextSlot.FRONT));
        // skippable clientbound packets: an encoder failure drops the packet instead of kicking
        packet("packet/system_chat_ok", pcb, new ClientboundSystemChatPacket(Component.literal("hi"), false));
        packet("packet/system_chat_long", pcb, new ClientboundSystemChatPacket(Component.literal(rep("a", 70000)), false));
        CompoundTag tq = new CompoundTag();
        tq.putString("k", rep("q", 70000));
        packet("packet/tag_query_long", pcb, new ClientboundTagQueryPacket(1, tq));

        W.close();
    }

    static BitSet bits(int... idx) {
        BitSet s = new BitSet();
        for (int i : idx) {
            if (i >= 0) s.set(i);
        }
        return s;
    }
}
