// Generates tests/fixtures/packets.golden: `label:hex` of vanilla's `ProtocolInfo.codec()` output (VarInt id + body).
// Run from the repo root:
//   scripts/vanilla-java.sh crates/mc-protocol/tests/fixtures/PacketGolden.java crates/mc-protocol/tests/fixtures/packets.golden
import com.mojang.authlib.GameProfile;
import com.mojang.authlib.properties.Property;
import com.mojang.authlib.properties.PropertyMap;
import com.google.common.collect.ImmutableMultimap;
import com.mojang.datafixers.util.Either;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.util.*;
import net.minecraft.ChatFormatting;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistrySynchronization;
import net.minecraft.nbt.*;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.Style;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.*;
import net.minecraft.network.protocol.common.custom.*;
import net.minecraft.network.protocol.configuration.*;
import net.minecraft.network.protocol.cookie.*;
import net.minecraft.network.protocol.handshake.*;
import net.minecraft.network.protocol.login.*;
import net.minecraft.network.protocol.login.custom.*;
import net.minecraft.network.protocol.ping.*;
import net.minecraft.network.protocol.status.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.ServerLinks;
import net.minecraft.server.dialog.Dialog;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.packs.repository.KnownPack;
import net.minecraft.server.players.NameAndId;
import net.minecraft.tags.TagNetworkSerialization;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.server.level.ParticleStatus;

public class PacketGolden {
    @SuppressWarnings({"rawtypes", "unchecked"})
    static void out(String label, ProtocolInfo<?> info, Packet<?> p) {
        ByteBuf buf = Unpooled.buffer();
        ((StreamCodec) info.codec()).encode(buf, p);
        W.println(label + ":" + ByteBufUtil.hexDump(buf));
    }

    static java.io.PrintWriter W;

    static Identifier id(String s) { return Identifier.parse(s); }

    public static void main(String[] args) throws Exception {
        W = new java.io.PrintWriter(args[0]);
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();

        // ---- handshake
        var hs = HandshakeProtocols.SERVERBOUND;
        out("handshake/intention_login", hs, new ClientIntentionPacket(777, "localhost", 25565, ClientIntent.LOGIN));
        out("handshake/intention_status", hs, new ClientIntentionPacket(-1, "mc.example.org", 1, ClientIntent.STATUS));
        out("handshake/intention_transfer", hs, new ClientIntentionPacket(777, "h\u00e9llo.\u00fcnicode", 65535, ClientIntent.TRANSFER));

        // ---- status
        var sc = StatusProtocols.CLIENTBOUND;
        var ss = StatusProtocols.SERVERBOUND;
        out("status/status_request", ss, ServerboundStatusRequestPacket.INSTANCE);
        out("status/ping_request", ss, new ServerboundPingRequestPacket(0x0123456789abcdefL));
        out("status/pong_response", sc, new ClientboundPongResponsePacket(-42L));
        var uuid1 = UUID.fromString("069a79f4-44e9-4726-a5be-fca90e38aaf5");
        var full = new ServerStatus(
            Component.literal("A Minecraft Server"),
            Optional.of(new ServerStatus.Players(20, 3, List.of(new NameAndId(uuid1, "Notch")))),
            Optional.of(new ServerStatus.Version("26.3", 777)),
            Optional.of(new ServerStatus.Favicon(new byte[]{1, 2, 3, 4, 5, (byte) 0xff, (byte) 0xfe})),
            true);
        out("status/status_response_full", sc, new ClientboundStatusResponsePacket(full));
        var min = new ServerStatus(Component.empty(), Optional.empty(), Optional.empty(), Optional.empty(), false);
        out("status/status_response_min", sc, new ClientboundStatusResponsePacket(min));
        var styled = new ServerStatus(
            Component.literal("Hello").withStyle(ChatFormatting.RED, ChatFormatting.BOLD).append(Component.translatable("menu.quit")),
            Optional.of(new ServerStatus.Players(0, 0, List.of())), Optional.empty(), Optional.empty(), false);
        out("status/status_response_styled", sc, new ClientboundStatusResponsePacket(styled));

        // ---- login
        var ls = LoginProtocols.SERVERBOUND;
        var lc = LoginProtocols.CLIENTBOUND;
        out("login/hello_sb", ls, new ServerboundHelloPacket("Steve", uuid1));
        out("login/hello_cb", lc, new ClientboundHelloPacket("", new byte[]{1, 2, 3, 4}, new byte[]{9, 8, 7, 6}, true));
        out("login/login_compression", lc, new ClientboundLoginCompressionPacket(256));
        out("login/login_disconnect", lc, new ClientboundLoginDisconnectPacket(Component.literal("bye").withStyle(ChatFormatting.GREEN)));
        var props = ImmutableMultimap.<String, Property>builder()
            .put("textures", new Property("textures", "dGV4dHVyZXM=", "c2lnbmF0dXJl"))
            .put("plain", new Property("plain", "v", null)).build();
        out("login/login_finished", lc, new ClientboundLoginFinishedPacket(new GameProfile(uuid1, "Notch", new PropertyMap(props)), UUID.fromString("00000000-0000-0001-0000-000000000002")));
        out("login/login_finished_noprops", lc, new ClientboundLoginFinishedPacket(new GameProfile(uuid1, "Notch"), uuid1));
        out("login/login_acknowledged", ls, ServerboundLoginAcknowledgedPacket.INSTANCE);
        out("login/custom_query", lc, new ClientboundCustomQueryPacket(300, new DiscardedQueryPayload(id("mod:chan"))));
        out("login/custom_query_answer_null", ls, new ServerboundCustomQueryAnswerPacket(5, null));
        out("login/custom_query_answer_some", ls, new ServerboundCustomQueryAnswerPacket(5, DiscardedQueryAnswerPayload.INSTANCE));
        out("login/cookie_request", lc, new ClientboundCookieRequestPacket(id("mod:cookie")));
        out("login/cookie_response_some", ls, new ServerboundCookieResponsePacket(id("mod:cookie"), new byte[]{1, 2, 3}));
        out("login/cookie_response_null", ls, new ServerboundCookieResponsePacket(id("mod:cookie"), null));

        // ---- configuration clientbound
        var cc = ConfigurationProtocols.CLIENTBOUND;
        var cs = ConfigurationProtocols.SERVERBOUND;
        out("config/cb_cookie_request", cc, new ClientboundCookieRequestPacket(id("minecraft:c")));
        out("config/cb_custom_payload_brand", cc, new ClientboundCustomPayloadPacket(new BrandPayload("vanilla")));
        out("config/cb_custom_payload_discarded", cc, new ClientboundCustomPayloadPacket(new DiscardedPayload(id("mod:x"))));
        out("config/cb_disconnect", cc, new ClientboundDisconnectPacket(Component.translatable("multiplayer.disconnect.kicked").withStyle(Style.EMPTY.withColor(ChatFormatting.RED).withBold(true))));
        out("config/cb_finish_configuration", cc, ClientboundFinishConfigurationPacket.INSTANCE);
        out("config/cb_keep_alive", cc, new ClientboundKeepAlivePacket(0x7fffffffffffffffL));
        out("config/cb_ping", cc, new ClientboundPingPacket(-7));
        out("config/cb_reset_chat", cc, ClientboundResetChatPacket.INSTANCE);
        var reg = new CompoundTag();
        reg.putString("asset_id", "minecraft:x");
        reg.putFloat("f", 1.5f);
        out("config/cb_registry_data", cc, new ClientboundRegistryDataPacket(ResourceKey.createRegistryKey(id("minecraft:dimension_type")), List.of(
            new RegistrySynchronization.PackedRegistryEntry(id("minecraft:overworld"), Optional.of(reg)),
            new RegistrySynchronization.PackedRegistryEntry(id("minecraft:the_nether"), Optional.empty()))));
        out("config/cb_resource_pack_pop_some", cc, new ClientboundResourcePackPopPacket(Optional.of(uuid1)));
        out("config/cb_resource_pack_pop_none", cc, new ClientboundResourcePackPopPacket(Optional.empty()));
        out("config/cb_resource_pack_push", cc, new ClientboundResourcePackPushPacket(uuid1, "https://example.org/p.zip", "0123456789abcdef0123456789abcdef01234567", true, Optional.of(Component.literal("Please"))));
        out("config/cb_resource_pack_push_noprompt", cc, new ClientboundResourcePackPushPacket(uuid1, "u", "", false, Optional.empty()));
        out("config/cb_post_effects", cc, new ClientboundPostEffectsPacket(List.of(id("minecraft:a"), id("b:c"))));
        out("config/cb_store_cookie", cc, new ClientboundStoreCookiePacket(id("mod:cookie"), new byte[]{5, 4, 3}));
        out("config/cb_transfer", cc, new ClientboundTransferPacket("play.example.org", 25566));
        out("config/cb_update_enabled_features", cc, new ClientboundUpdateEnabledFeaturesPacket(Set.of(id("minecraft:vanilla"))));
        out("config/cb_update_tags", cc, new ClientboundUpdateTagsPacket(Map.of(
            ResourceKey.createRegistryKey(id("minecraft:block")), new TagNetworkSerialization.NetworkPayload(Map.of(id("minecraft:logs"), new it.unimi.dsi.fastutil.ints.IntArrayList(new int[]{1, 200, 70000}))))));
        out("config/cb_select_known_packs", cc, new ClientboundSelectKnownPacks(List.of(new KnownPack("minecraft", "core", "26.3"), new KnownPack("mod", "p", "1"))));
        out("config/cb_custom_report_details", cc, new ClientboundCustomReportDetailsPacket(Map.of("k", "v")));
        out("config/cb_server_links", cc, new ClientboundServerLinksPacket(List.of(
            new ServerLinks.UntrustedEntry(Either.left(ServerLinks.KnownLinkType.WEBSITE), "https://example.org"),
            new ServerLinks.UntrustedEntry(Either.right(Component.literal("Wiki")), "https://wiki.example.org"))));
        out("config/cb_clear_dialog", cc, ClientboundClearDialogPacket.INSTANCE);
        var dialogTag = TagParser.parseCompoundFully("{type:\"minecraft:notice\",title:\"Hello\"}");
        Dialog dialog = Dialog.DIRECT_CODEC.parse(NbtOps.INSTANCE, dialogTag).getOrThrow();
        out("config/cb_show_dialog", cc, new ClientboundShowDialogPacket(Holder.direct(dialog)));
        out("config/cb_code_of_conduct", cc, new ClientboundCodeOfConductPacket("Be nice."));

        // ---- configuration serverbound
        out("config/sb_client_information_default", cs, new ServerboundClientInformationPacket(ClientInformation.createDefault()));
        out("config/sb_client_information", cs, new ServerboundClientInformationPacket(new ClientInformation("de_de", 12, ChatVisiblity.HIDDEN, false, 0x7f, HumanoidArm.LEFT, true, true, ParticleStatus.MINIMAL)));
        out("config/sb_cookie_response", cs, new ServerboundCookieResponsePacket(id("minecraft:c"), new byte[0]));
        out("config/sb_custom_payload_brand", cs, new ServerboundCustomPayloadPacket(new BrandPayload("fabric")));
        out("config/sb_finish_configuration", cs, ServerboundFinishConfigurationPacket.INSTANCE);
        out("config/sb_keep_alive", cs, new ServerboundKeepAlivePacket(-1L));
        out("config/sb_pong", cs, new ServerboundPongPacket(123456));
        out("config/sb_resource_pack", cs, new ServerboundResourcePackPacket(uuid1, ServerboundResourcePackPacket.Action.FAILED_RELOAD));
        out("config/sb_select_known_packs", cs, new ServerboundSelectKnownPacks(List.of(new KnownPack("minecraft", "core", "26.3"))));
        var click = new CompoundTag();
        click.putInt("n", 5);
        out("config/sb_custom_click_action_some", cs, new ServerboundCustomClickActionPacket(id("mod:click"), Optional.of(click)));
        out("config/sb_custom_click_action_none", cs, new ServerboundCustomClickActionPacket(id("mod:click"), Optional.empty()));
        out("config/sb_accept_code_of_conduct", cs, ServerboundAcceptCodeOfConductPacket.INSTANCE);
        W.close();
    }
}
