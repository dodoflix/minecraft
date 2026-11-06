// Generates tests/fixtures/codec.golden: `label:hex` of vanilla FriendlyByteBuf / ByteBufCodecs encodings.
// Usage: scripts/vanilla-java.sh -o crates/mc-protocol/tests/fixtures/codec.golden crates/mc-protocol/tests/fixtures/CodecGolden.java crates/mc-protocol/tests/fixtures/codec.golden
// Output: crates/mc-protocol/tests/fixtures/codec.golden
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.GlobalPos;
import net.minecraft.core.SectionPos;
import net.minecraft.core.Vec3i;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.ServerLinks;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.Vec3;

public class CodecGolden {
    static java.io.PrintWriter W;

    static <T> void out(String label, StreamCodec<? super ByteBuf, T> codec, T value) {
        ByteBuf buf = Unpooled.buffer();
        codec.encode(buf, value);
        W.println(label + ":" + ByteBufUtil.hexDump(buf));
    }

    static void raw(String label, java.util.function.Consumer<FriendlyByteBuf> f) {
        FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.buffer());
        f.accept(buf);
        W.println(label + ":" + ByteBufUtil.hexDump(buf));
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        W = new java.io.PrintWriter(args[0]);

        double[][] lp = {
            {0, 0, 0}, {1e-6, 0, 0}, {0.5, -0.25, 0.125}, {1, 1, 1}, {-1, 2, -3}, {3, 3, 3}, {4, 0, 0},
            {100.5, -2000.25, 7}, {1.7179869183E10, -1.7179869183E10, 0}, {1e12, 0, -1e12}, {Double.NaN, 5, 0},
            {0.00003051944088384301, 0, 0}, {123456.789, 0.001, -99999.9999}, {-0.0, 3.0517578125E-5, 0},
        };
        for (int i = 0; i < lp.length; i++) {
            out("lpvec3/" + i, Vec3.LP_STREAM_CODEC, new Vec3(lp[i][0], lp[i][1], lp[i][2]));
            ByteBuf enc = Unpooled.buffer();
            Vec3.LP_STREAM_CODEC.encode(enc, new Vec3(lp[i][0], lp[i][1], lp[i][2]));
            Vec3 dec = Vec3.LP_STREAM_CODEC.decode(enc);
            W.println("lpvec3_decoded/" + i + ":" + String.format("%016x%016x%016x", Double.doubleToLongBits(dec.x), Double.doubleToLongBits(dec.y), Double.doubleToLongBits(dec.z)));
        }
        out("vec3/0", Vec3.STREAM_CODEC, new Vec3(1.5, -2.25, 1e300));
        out("vec3i/0", Vec3i.STREAM_CODEC, new Vec3i(1, -1, 300000));
        int[][] bp = {{0, 0, 0}, {1, 2, 3}, {-1, -1, -1}, {33554431, 2047, -33554432}, {-33554432, -2048, 33554431}, {123456, -64, -654321}};
        for (int i = 0; i < bp.length; i++) {
            out("blockpos/" + i, BlockPos.STREAM_CODEC, new BlockPos(bp[i][0], bp[i][1], bp[i][2]));
            out("sectionpos/" + i, SectionPos.STREAM_CODEC, SectionPos.of(bp[i][0] >> 4, bp[i][1] >> 4, bp[i][2] >> 4));
            out("chunkpos/" + i, ChunkPos.STREAM_CODEC, new ChunkPos(bp[i][0], bp[i][2]));
        }
        out("globalpos/0", GlobalPos.STREAM_CODEC, GlobalPos.of(ResourceKey.create(net.minecraft.core.registries.Registries.DIMENSION, Identifier.parse("minecraft:the_nether")), new BlockPos(-5, 70, 12)));
        float[] ang = {0f, 90f, -90f, 180f, 359.9f, 360f, -0.1f, 1000f, 1.4f, -720.5f};
        for (int i = 0; i < ang.length; i++) {
            out("angle/" + i, ByteBufCodecs.ROTATION_BYTE, ang[i]);
        }
        int[] vi = {0, 1, 127, 128, 255, 25565, 2097151, Integer.MAX_VALUE, -1, Integer.MIN_VALUE};
        for (int i = 0; i < vi.length; i++) out("varint/" + i, ByteBufCodecs.VAR_INT, vi[i]);
        long[] vl = {0, 1, 127, 128, Long.MAX_VALUE, -1, Long.MIN_VALUE, 1L << 35};
        for (int i = 0; i < vl.length; i++) out("varlong/" + i, ByteBufCodecs.VAR_LONG, vl[i]);
        String[] strs = {"", "abc", "h\u00e9llo \u20ac", "\ud83d\ude00 emoji", "\u0000nul"};
        for (int i = 0; i < strs.length; i++) out("string/" + i, ByteBufCodecs.STRING_UTF8, strs[i]);
        out("optvarint/none", ByteBufCodecs.OPTIONAL_VAR_INT, OptionalInt.empty());
        out("optvarint/some", ByteBufCodecs.OPTIONAL_VAR_INT, OptionalInt.of(41));
        out("optvarint/max", ByteBufCodecs.OPTIONAL_VAR_INT, OptionalInt.of(Integer.MAX_VALUE));
        out("rgb/0", ByteBufCodecs.RGB_COLOR, 0xFF336699);
        out("instant/0", ByteBufCodecs.INSTANT, java.time.Instant.ofEpochMilli(1700000000123L));
        out("uuid/0", net.minecraft.core.UUIDUtil.STREAM_CODEC, UUID.fromString("069a79f4-44e9-4726-a5be-fca90e38aaf5"));
        BitSet bs = new BitSet(); bs.set(1); bs.set(70); bs.set(130);
        raw("bitset/long", b -> b.writeBitSet(bs));
        out("bitset/bytes", ByteBufCodecs.BIT_SET, bs);
        raw("bitset/empty_long", b -> b.writeBitSet(new BitSet()));
        out("bitset/empty_bytes", ByteBufCodecs.BIT_SET, new BitSet());
        BitSet fb = new BitSet(); fb.set(0); fb.set(17);
        out("bitset/fixed20", ByteBufCodecs.fixedBitSet(20), fb);
        EnumSet<ServerLinks.KnownLinkType> es = EnumSet.of(ServerLinks.KnownLinkType.SUPPORT, ServerLinks.KnownLinkType.ANNOUNCEMENTS);
        raw("enumset/0", b -> b.writeEnumSet(es, ServerLinks.KnownLinkType.class));
        raw("longarray/0", b -> b.writeLongArray(new long[]{1, -2, Long.MAX_VALUE}));
        out("bytearray/0", ByteBufCodecs.BYTE_ARRAY, new byte[]{1, 2, 3});
        out("list/0", ByteBufCodecs.VAR_INT.apply(ByteBufCodecs.list()), List.of(1, 300, -1));
        out("optional/some", ByteBufCodecs.optional(ByteBufCodecs.INT), Optional.of(7));
        out("optional/none", ByteBufCodecs.optional(ByteBufCodecs.INT), Optional.empty());
        out("either/left", ByteBufCodecs.either(ByteBufCodecs.INT, ByteBufCodecs.STRING_UTF8), com.mojang.datafixers.util.Either.left(9));
        out("either/right", ByteBufCodecs.either(ByteBufCodecs.INT, ByteBufCodecs.STRING_UTF8), com.mojang.datafixers.util.Either.right("x"));
        out("nbt/compound", ByteBufCodecs.COMPOUND_TAG, net.minecraft.nbt.TagParser.parseCompoundFully("{a:1b,b:[1,2,3],c:{d:\"e\"},f:[I;1,2],g:1.5d}"));
        W.close();
    }
}
