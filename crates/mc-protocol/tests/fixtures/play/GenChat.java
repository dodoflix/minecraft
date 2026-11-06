// Golden generator for the play `chat` packet group (chat.golden): see PlayHarness.java / gen.sh.
// Usage: crates/mc-protocol/tests/fixtures/play/gen.sh GenChat.java
// Output: crates/mc-protocol/tests/fixtures/play/chat.golden.gz
// Besides packet lines (`label:id+body`) it writes, for decode checks against the real vanilla codecs:
//   `<label>` + `<label>_err`: bytes vanilla refuses to decode, and the exception message (as a writeUtf string);
//   `<label>` + `<label>_re`:  bytes vanilla accepts and the bytes it encodes back from what it decoded.
import com.google.common.collect.ImmutableMultimap;
import com.mojang.authlib.GameProfile;
import com.mojang.authlib.properties.Property;
import com.mojang.authlib.properties.PropertyMap;
import com.mojang.brigadier.Command;
import com.mojang.brigadier.arguments.*;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.brigadier.builder.RequiredArgumentBuilder;
import com.mojang.brigadier.tree.ArgumentCommandNode;
import com.mojang.brigadier.tree.CommandNode;
import com.mojang.brigadier.tree.LiteralCommandNode;
import com.mojang.brigadier.tree.RootCommandNode;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.lang.reflect.Field;
import java.security.KeyPair;
import java.security.KeyPairGenerator;
import java.util.*;
import java.util.function.Consumer;
import net.minecraft.ChatFormatting;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.*;
import net.minecraft.commands.arguments.blocks.BlockPredicateArgument;
import net.minecraft.commands.arguments.blocks.BlockStateArgument;
import net.minecraft.commands.arguments.coordinates.*;
import net.minecraft.commands.arguments.item.FunctionArgument;
import net.minecraft.commands.arguments.item.ItemArgument;
import net.minecraft.commands.arguments.item.ItemPredicateArgument;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.*;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.GameType;

public class GenChat {
   static final Identifier id(String s) { return Identifier.parse(s); }

   // ---- helpers -------------------------------------------------------------------------------------------------

   static byte[] bytes(Consumer<RegistryFriendlyByteBuf> w) {
      RegistryFriendlyByteBuf b = new RegistryFriendlyByteBuf(Unpooled.buffer(), PlayHarness.access);
      w.accept(b);
      return ByteBufUtil.getBytes(b);
   }

   static void rawBytes(String label, byte[] bytes) {
      PlayHarness.raw(label, b -> b.writeBytes(bytes));
   }

   @SuppressWarnings({"rawtypes", "unchecked"})
   static Object decode(StreamCodec codec, byte[] bytes) {
      RegistryFriendlyByteBuf in = new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(bytes), PlayHarness.access);
      Object v = codec.decode(in);
      if (in.isReadable()) throw new IllegalStateException(in.readableBytes() + " trailing bytes");
      return v;
   }

   /** Vanilla must reject `bytes`; stores the message. */
   @SuppressWarnings({"rawtypes", "unchecked"})
   static void err(String label, StreamCodec codec, byte[] bytes) {
      String msg;
      try {
         decode(codec, bytes);
         throw new AssertionError(label + ": vanilla accepted the bytes");
      } catch (AssertionError e) {
         throw e;
      } catch (Throwable t) {
         msg = String.valueOf(t.getMessage());
      }
      rawBytes(label, bytes);
      String m = msg;
      PlayHarness.raw(label + "_err", b -> b.writeUtf(m, 1 << 20));
   }

   /** Vanilla accepts `bytes`; stores what it encodes back. */
   @SuppressWarnings({"rawtypes", "unchecked"})
   static void lossy(String label, StreamCodec codec, byte[] bytes) {
      Object v = decode(codec, bytes);
      rawBytes(label, bytes);
      PlayHarness.raw(label + "_re", b -> codec.encode(b, v));
   }

   static MessageSignature sig(int seed) {
      byte[] b = new byte[256];
      for (int i = 0; i < 256; i++) b[i] = (byte)(seed * 31 + i * 7);
      return new MessageSignature(b);
   }

   static String rep(String s, int n) { return s.repeat(n); }

   static Component[] COMPS;
   static void initComps() {
   COMPS = new Component[]{
      Component.empty(),
      Component.literal("hi"),
      Component.literal("h\u00e9llo \uD83D\uDE00 w\u00f6rld \u0000\u2028"),
      Component.translatable("chat.type.text", "a", Component.literal("b")),
      Component.literal("red").withStyle(ChatFormatting.RED, ChatFormatting.BOLD),
      Component.literal("a").append(Component.literal("b").withStyle(Style.EMPTY.withColor(0x336699).withItalic(true))).append("c"),
      Component.literal("tip").withStyle(Style.EMPTY.withHoverEvent(new HoverEvent.ShowText(Component.literal("hover")))),
      Component.keybind("key.jump"),
   };
   }

   static Holder<ChatType> chatType(net.minecraft.resources.ResourceKey<ChatType> k) {
      return PlayHarness.access.lookupOrThrow(Registries.CHAT_TYPE).getOrThrow(k);
   }

   static List<net.minecraft.resources.ResourceKey<ChatType>> CHAT_TYPES;

   static ChatTypeDecoration decoration(String key, Style style, ChatTypeDecoration.Parameter... params) {
      return new ChatTypeDecoration(key, List.of(params), style);
   }

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);
      initComps();
      CHAT_TYPES = List.of(
         ChatType.CHAT, ChatType.SAY_COMMAND, ChatType.MSG_COMMAND_INCOMING, ChatType.MSG_COMMAND_OUTGOING,
         ChatType.TEAM_MSG_COMMAND_INCOMING, ChatType.TEAM_MSG_COMMAND_OUTGOING, ChatType.EMOTE_COMMAND);
      MessageSignature s1 = sig(1), s2 = sig(2);

      // chat type registry ids, in CHAT_TYPES order (one byte each)
      PlayHarness.raw("chat_type_ids", b -> {
         for (var k : CHAT_TYPES) {
            int i = PlayHarness.access.lookupOrThrow(Registries.CHAT_TYPE).getId(chatType(k).value());
            b.writeByte(i);
         }
      });

      // ---- nested types ----------------------------------------------------------------------------------------
      for (int id : new int[]{0, 1, 126, 127, 128, 1000, Integer.MAX_VALUE - 1})
         PlayHarness.raw("sigpacked/cached_" + id, b -> MessageSignature.Packed.STREAM_CODEC.encode(b, new MessageSignature.Packed(id)));
      PlayHarness.raw("sigpacked/full", b -> MessageSignature.Packed.STREAM_CODEC.encode(b, new MessageSignature.Packed(s1)));
      // negative ids decode (and re-encode) as they come
      lossy("sigpacked/negative", MessageSignature.Packed.STREAM_CODEC, bytes(b -> b.writeVarInt(-5)));

      PlayHarness.raw("lastseen/empty", b -> LastSeenMessages.Packed.STREAM_CODEC.encode(b, LastSeenMessages.Packed.EMPTY));
      PlayHarness.raw("lastseen/mixed", b -> LastSeenMessages.Packed.STREAM_CODEC.encode(b, new LastSeenMessages.Packed(List.of(
         new MessageSignature.Packed(3), new MessageSignature.Packed(s1), new MessageSignature.Packed(127)))));
      List<MessageSignature.Packed> twenty = new ArrayList<>();
      for (int i = 0; i < 20; i++) twenty.add(new MessageSignature.Packed(i * 6));
      PlayHarness.raw("lastseen/twenty", b -> LastSeenMessages.Packed.STREAM_CODEC.encode(b, new LastSeenMessages.Packed(twenty)));
      err("lastseen/twentyone", LastSeenMessages.Packed.STREAM_CODEC, bytes(b -> { b.writeVarInt(21); for (int i = 0; i < 21; i++) b.writeVarInt(i + 1); }));

      for (int mask : new int[]{0, 1, 0x80, 0xfffff, 0x55555, 0x12345}) {
         BitSet bs = BitSet.valueOf(new long[]{mask});
         for (int off : new int[]{0, 5, 300, -1}) for (byte ck : new byte[]{0, 1, -1, 127}) {
            PlayHarness.raw("update/" + mask + "_" + off + "_" + ck, b -> LastSeenMessages.Update.STREAM_CODEC.encode(b, new LastSeenMessages.Update(off, bs, ck)));
         }
      }
      // padding bits 20..23 are read into the set
      // (vanilla decodes it, but cannot encode the set back: BitSet is larger than expected size)
      byte[] padded = bytes(b -> { b.writeVarInt(1); b.writeBytes(new byte[]{0x01, 0x00, (byte)0xf8}); b.writeByte(3); });
      decode(LastSeenMessages.Update.STREAM_CODEC, padded);
      rawBytes("update/padding", padded);

      PlayHarness.raw("filtermask/pass", b -> FilterMask.STREAM_CODEC.encode(b, FilterMask.PASS_THROUGH));
      PlayHarness.raw("filtermask/full", b -> FilterMask.STREAM_CODEC.encode(b, FilterMask.FULLY_FILTERED));
      PlayHarness.raw("filtermask/partial_empty", b -> FilterMask.STREAM_CODEC.encode(b, new FilterMask(0)));
      for (int[] bits : new int[][]{{0}, {1, 2, 3}, {7, 8}, {63, 64}, {100, 200, 255}, {1000}}) {
         FilterMask m = new FilterMask(0);
         for (int i : bits) m.setFiltered(i);
         PlayHarness.raw("filtermask/partial_" + bits[0] + "_" + bits.length, b -> FilterMask.STREAM_CODEC.encode(b, m));
      }
      lossy("filtermask/unknown_type", FilterMask.STREAM_CODEC, bytes(b -> b.writeVarInt(9)));
      lossy("filtermask/negative_type", FilterMask.STREAM_CODEC, bytes(b -> b.writeVarInt(-1)));

      PlayHarness.raw("body/basic", b -> SignedMessageBody.Packed.STREAM_CODEC.encode(b,
         new SignedMessageBody.Packed("hello", java.time.Instant.ofEpochMilli(1700000000123L), 42L, LastSeenMessages.Packed.EMPTY)));
      PlayHarness.raw("body/extreme", b -> SignedMessageBody.Packed.STREAM_CODEC.encode(b,
         new SignedMessageBody.Packed(rep("\uD83D\uDE00", 128), java.time.Instant.ofEpochMilli(-1L), Long.MIN_VALUE,
            new LastSeenMessages.Packed(List.of(new MessageSignature.Packed(s2), new MessageSignature.Packed(0))))));
      PlayHarness.raw("body/max_len", b -> SignedMessageBody.Packed.STREAM_CODEC.encode(b,
         new SignedMessageBody.Packed(rep("x", 256), java.time.Instant.EPOCH, 0L, LastSeenMessages.Packed.EMPTY)));
      err("body/too_long", SignedMessageBody.Packed.STREAM_CODEC, bytes(b -> { b.writeUtf(rep("x", 257), 32767); b.writeLong(0); b.writeLong(0); b.writeVarInt(0); }));
      err("body/too_long_utf16", SignedMessageBody.Packed.STREAM_CODEC, bytes(b -> { b.writeUtf(rep("\uD83D\uDE00", 129), 32767); b.writeLong(0); b.writeLong(0); b.writeVarInt(0); }));

      // ---- chat types ------------------------------------------------------------------------------------------
      ChatTypeDecoration d1 = ChatTypeDecoration.withSender("chat.type.text");
      ChatTypeDecoration d2 = decoration("x.y", Style.EMPTY.withColor(ChatFormatting.GRAY).withItalic(true),
         ChatTypeDecoration.Parameter.TARGET, ChatTypeDecoration.Parameter.SENDER, ChatTypeDecoration.Parameter.CONTENT);
      ChatTypeDecoration d3 = decoration("", Style.EMPTY);
      PlayHarness.raw("decoration/sender", b -> ChatTypeDecoration.STREAM_CODEC.encode(b, d1));
      PlayHarness.raw("decoration/team", b -> ChatTypeDecoration.STREAM_CODEC.encode(b, d2));
      PlayHarness.raw("decoration/empty", b -> ChatTypeDecoration.STREAM_CODEC.encode(b, d3));
      PlayHarness.raw("decoration/unicode", b -> ChatTypeDecoration.STREAM_CODEC.encode(b, decoration("k\u00e9y.\uD83D\uDE00", Style.EMPTY.withBold(true), ChatTypeDecoration.Parameter.CONTENT)));
      lossy("decoration/unknown_param", ChatTypeDecoration.STREAM_CODEC, bytes(b -> { b.writeUtf("k"); b.writeVarInt(2); b.writeVarInt(7); b.writeVarInt(-1); b.writeNbt(new net.minecraft.nbt.CompoundTag()); }));
      PlayHarness.raw("chattype/direct", b -> ChatType.DIRECT_STREAM_CODEC.encode(b, new ChatType(d1, d2)));

      // ---- player_chat -----------------------------------------------------------------------------------------
      UUID sender = UUID.fromString("00112233-4455-6677-8899-aabbccddeeff");
      var body = new SignedMessageBody.Packed("hello world", java.time.Instant.ofEpochMilli(1700000000000L), 123456789L,
         new LastSeenMessages.Packed(List.of(new MessageSignature.Packed(0), new MessageSignature.Packed(s2))));
      var body0 = new SignedMessageBody.Packed("", java.time.Instant.EPOCH, 0L, LastSeenMessages.Packed.EMPTY);
      for (int i = 0; i < CHAT_TYPES.size(); i++) {
         var bound = new ChatType.Bound(chatType(CHAT_TYPES.get(i)), COMPS[i % COMPS.length], i % 2 == 0 ? Optional.empty() : Optional.of(COMPS[(i + 3) % COMPS.length]));
         PlayHarness.cb("player_chat/type_" + i, new ClientboundPlayerChatPacket(i, sender, i * 10, Optional.of(s1), body, Optional.empty(), FilterMask.PASS_THROUGH, bound));
      }
      var bound0 = new ChatType.Bound(chatType(ChatType.CHAT), Component.literal("Steve"), Optional.empty());
      PlayHarness.cb("player_chat/unsigned", new ClientboundPlayerChatPacket(0, sender, 0, Optional.empty(), body0, Optional.empty(), FilterMask.PASS_THROUGH, bound0));
      PlayHarness.cb("player_chat/unsigned_content", new ClientboundPlayerChatPacket(1, sender, 1, Optional.empty(), body0, Optional.of(Component.literal("unsigned \uD83D\uDE00").withStyle(ChatFormatting.GREEN)), FilterMask.PASS_THROUGH, bound0));
      PlayHarness.cb("player_chat/signed_content", new ClientboundPlayerChatPacket(2, sender, 2, Optional.of(s1), body, Optional.of(COMPS[3]), FilterMask.PASS_THROUGH, bound0));
      PlayHarness.cb("player_chat/fully_filtered", new ClientboundPlayerChatPacket(3, sender, 3, Optional.of(s2), body, Optional.empty(), FilterMask.FULLY_FILTERED, bound0));
      FilterMask part = new FilterMask(0);
      part.setFiltered(0); part.setFiltered(5); part.setFiltered(70);
      PlayHarness.cb("player_chat/partially_filtered", new ClientboundPlayerChatPacket(4, sender, 4, Optional.of(s2), body, Optional.empty(), part, bound0));
      PlayHarness.cb("player_chat/partially_filtered_empty", new ClientboundPlayerChatPacket(4, sender, 4, Optional.of(s2), body, Optional.empty(), new FilterMask(0), bound0));
      PlayHarness.cb("player_chat/extreme_indices", new ClientboundPlayerChatPacket(Integer.MAX_VALUE, new UUID(Long.MIN_VALUE, Long.MAX_VALUE), -1, Optional.of(s1), body, Optional.empty(), FilterMask.PASS_THROUGH, bound0));
      PlayHarness.cb("player_chat/negative_global", new ClientboundPlayerChatPacket(Integer.MIN_VALUE, sender, Integer.MIN_VALUE, Optional.empty(), body0, Optional.empty(), FilterMask.PASS_THROUGH, bound0));
      var direct = new ChatType.Bound(Holder.direct(new ChatType(d1, d2)), COMPS[2], Optional.of(COMPS[4]));
      PlayHarness.cb("player_chat/direct_chat_type", new ClientboundPlayerChatPacket(5, sender, 5, Optional.of(s1), body, Optional.of(COMPS[5]), part, direct));
      PlayHarness.cb("player_chat/unicode", new ClientboundPlayerChatPacket(6, sender, 6, Optional.of(s1),
         new SignedMessageBody.Packed("\uD83D\uDE00 caf\u00e9 \u4e2d\u6587", java.time.Instant.ofEpochMilli(1L), -7L, LastSeenMessages.Packed.EMPTY),
         Optional.of(COMPS[2]), FilterMask.PASS_THROUGH, new ChatType.Bound(chatType(ChatType.EMOTE_COMMAND), COMPS[2], Optional.of(COMPS[2]))));

      for (int i = 0; i < COMPS.length; i++) {
         PlayHarness.cb("disguised_chat/" + i, new ClientboundDisguisedChatPacket(COMPS[i], new ChatType.Bound(chatType(CHAT_TYPES.get(i % 7)), COMPS[(i + 1) % COMPS.length], i % 2 == 0 ? Optional.empty() : Optional.of(COMPS[i]))));
      }
      PlayHarness.cb("disguised_chat/direct", new ClientboundDisguisedChatPacket(COMPS[3], direct));

      for (int id : new int[]{0, 1, 127, 128, 16383})
         PlayHarness.cb("delete_chat/cached_" + id, new ClientboundDeleteChatPacket(new MessageSignature.Packed(id)));
      PlayHarness.cb("delete_chat/full", new ClientboundDeleteChatPacket(new MessageSignature.Packed(s1)));

      for (int i = 0; i < COMPS.length; i++) {
         PlayHarness.cb("system_chat/" + i + "_chat", new ClientboundSystemChatPacket(COMPS[i], false));
         PlayHarness.cb("system_chat/" + i + "_overlay", new ClientboundSystemChatPacket(COMPS[i], true));
      }
      PlayHarness.cb("tab_list/empty", new ClientboundTabListPacket(Component.empty(), Component.empty()));
      PlayHarness.cb("tab_list/both", new ClientboundTabListPacket(COMPS[4], COMPS[5]));
      PlayHarness.cb("tab_list/unicode", new ClientboundTabListPacket(COMPS[2], COMPS[3]));
      PlayHarness.cb("clear_titles/keep", new ClientboundClearTitlesPacket(false));
      PlayHarness.cb("clear_titles/reset", new ClientboundClearTitlesPacket(true));
      for (int i = 0; i < COMPS.length; i++) {
         PlayHarness.cb("set_title_text/" + i, new ClientboundSetTitleTextPacket(COMPS[i]));
         PlayHarness.cb("set_subtitle_text/" + i, new ClientboundSetSubtitleTextPacket(COMPS[i]));
         PlayHarness.cb("set_action_bar_text/" + i, new ClientboundSetActionBarTextPacket(COMPS[i]));
      }
      PlayHarness.cb("set_titles_animation/default", new ClientboundSetTitlesAnimationPacket(10, 70, 20));
      PlayHarness.cb("set_titles_animation/zero", new ClientboundSetTitlesAnimationPacket(0, 0, 0));
      PlayHarness.cb("set_titles_animation/extreme", new ClientboundSetTitlesAnimationPacket(Integer.MAX_VALUE, -1, Integer.MIN_VALUE));

      PlayHarness.cb("command_suggestions/empty", new ClientboundCommandSuggestionsPacket(0, 0, 0, List.of()));
      PlayHarness.cb("command_suggestions/entries", new ClientboundCommandSuggestionsPacket(7, 4, 3, List.of(
         new ClientboundCommandSuggestionsPacket.Entry("say", Optional.empty()),
         new ClientboundCommandSuggestionsPacket.Entry("tellraw", Optional.of(COMPS[3])),
         new ClientboundCommandSuggestionsPacket.Entry("caf\u00e9 \uD83D\uDE00", Optional.of(COMPS[6])),
         new ClientboundCommandSuggestionsPacket.Entry("", Optional.empty()))));
      PlayHarness.cb("command_suggestions/extreme", new ClientboundCommandSuggestionsPacket(Integer.MAX_VALUE, -1, Integer.MIN_VALUE, List.of(new ClientboundCommandSuggestionsPacket.Entry(rep("a", 1000), Optional.empty()))));

      for (var a : ClientboundCustomChatCompletionsPacket.Action.values()) {
         PlayHarness.cb("custom_chat_completions/" + a + "_empty", new ClientboundCustomChatCompletionsPacket(a, List.of()));
         PlayHarness.cb("custom_chat_completions/" + a + "_some", new ClientboundCustomChatCompletionsPacket(a, List.of("alpha", "b\u00e9ta", "\uD83D\uDE00", "")));
      }
      List<String> many = new ArrayList<>();
      for (int i = 0; i < 300; i++) many.add("entry" + i);
      PlayHarness.cb("custom_chat_completions/many", new ClientboundCustomChatCompletionsPacket(ClientboundCustomChatCompletionsPacket.Action.SET, many));
      lossy("custom_chat_completions/unknown_action", ClientboundCustomChatCompletionsPacket.STREAM_CODEC, bytes(b -> { b.writeVarInt(9); b.writeVarInt(1); b.writeUtf("a"); }));

      // ---- serverbound -----------------------------------------------------------------------------------------
      for (int off : new int[]{0, 1, 127, 128, 16384, -1, Integer.MAX_VALUE, Integer.MIN_VALUE})
         PlayHarness.sb("chat_ack/" + off, new ServerboundChatAckPacket(off));
      String[] cmdTexts = {"", "say hi", "tp @s ~ ~ ~", "say \uD83D\uDE00 caf\u00e9", rep("a", 32767)};
      for (int i = 0; i < cmdTexts.length; i++) PlayHarness.sb("chat_command/" + i, new ServerboundChatCommandPacket(cmdTexts[i]));
      err("chat_command/too_long", ServerboundChatCommandPacket.STREAM_CODEC, bytes(b -> b.writeUtf(rep("a", 32768), 40000)));

      var emptySigs = ArgumentSignatures.EMPTY;
      List<ArgumentSignatures.Entry> eight = new ArrayList<>();
      for (int i = 0; i < 8; i++) eight.add(new ArgumentSignatures.Entry(rep("n", i + 9), sig(i + 10)));
      var upd0 = new LastSeenMessages.Update(0, new BitSet(), (byte)0);
      var bits = new BitSet(); bits.set(0); bits.set(3); bits.set(19);
      var upd1 = new LastSeenMessages.Update(17, bits, (byte)-77);
      PlayHarness.sb("chat_command_signed/empty", new ServerboundChatCommandSignedPacket("say hi", java.time.Instant.ofEpochMilli(1700000000000L), 99L, emptySigs, upd0));
      PlayHarness.sb("chat_command_signed/one", new ServerboundChatCommandSignedPacket("msg a hi", java.time.Instant.ofEpochMilli(1L), -1L, new ArgumentSignatures(List.of(new ArgumentSignatures.Entry("message", s1))), upd1));
      PlayHarness.sb("chat_command_signed/eight", new ServerboundChatCommandSignedPacket("x \uD83D\uDE00", java.time.Instant.ofEpochMilli(Long.MIN_VALUE / 1000000), Long.MAX_VALUE, new ArgumentSignatures(eight), upd1));
      PlayHarness.sb("chat_command_signed/name16", new ServerboundChatCommandSignedPacket("", java.time.Instant.EPOCH, 0L, new ArgumentSignatures(List.of(new ArgumentSignatures.Entry(rep("\uD83D\uDE00", 8), s2))), upd0));
      err("argsigs/nine", ArgumentSignatures.STREAM_CODEC, bytes(b -> { b.writeVarInt(9); for (int i = 0; i < 9; i++) { b.writeUtf("n"); b.writeBytes(sig(i).bytes()); } }));
      err("argsigs/name17", ArgumentSignatures.STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf(rep("n", 17)); b.writeBytes(sig(0).bytes()); }));
      err("argsigs/name17_utf16", ArgumentSignatures.STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf(rep("\uD83D\uDE00", 9)); b.writeBytes(sig(0).bytes()); }));
      err("argsigs/short_signature", ArgumentSignatures.STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf("n"); b.writeBytes(new byte[100]); }));

      var t0 = java.time.Instant.ofEpochMilli(1700000000000L);
      PlayHarness.sb("chat/unsigned", new ServerboundChatPacket("hello", t0, 0L, Optional.empty(), upd0));
      PlayHarness.sb("chat/signed", new ServerboundChatPacket("hello", t0, 5L, Optional.of(s1), upd1));
      PlayHarness.sb("chat/empty", new ServerboundChatPacket("", java.time.Instant.EPOCH, Long.MIN_VALUE, Optional.empty(), upd0));
      PlayHarness.sb("chat/max_len", new ServerboundChatPacket(rep("m", 256), t0, Long.MAX_VALUE, Optional.of(s2), upd1));
      PlayHarness.sb("chat/unicode", new ServerboundChatPacket("\uD83D\uDE00 caf\u00e9 \u4e2d" + rep("\uD83D\uDE00", 100), t0, -9L, Optional.empty(), upd1));
      err("chat/too_long", ServerboundChatPacket.STREAM_CODEC, bytes(b -> b.writeUtf(rep("m", 257), 32767)));
      err("chat/too_long_utf16", ServerboundChatPacket.STREAM_CODEC, bytes(b -> b.writeUtf(rep("\uD83D\uDE00", 129), 32767)));
      err("chat/too_long_bytes", ServerboundChatPacket.STREAM_CODEC, bytes(b -> b.writeUtf(rep("\u4e2d", 257), 32767)));

      // chat session / profile public key
      KeyPairGenerator kpg = KeyPairGenerator.getInstance("RSA");
      kpg.initialize(2048);
      KeyPair kp = kpg.generateKeyPair();
      byte[] der = kp.getPublic().getEncoded();
      rawBytes("pubkey_der", der);
      byte[] ksig = new byte[256];
      for (int i = 0; i < ksig.length; i++) ksig[i] = (byte)(i * 3);
      UUID session = UUID.fromString("a0a1a2a3-b0b1-c0c1-d0d1-e0e1e2e3e4e5");
      var data = new net.minecraft.world.entity.player.ProfilePublicKey.Data(java.time.Instant.ofEpochMilli(1800000000000L), kp.getPublic(), ksig);
      PlayHarness.raw("session_data", b -> RemoteChatSession.Data.STREAM_CODEC.encode(b, new RemoteChatSession.Data(session, data)));
      PlayHarness.sb("chat_session_update/basic", new ServerboundChatSessionUpdatePacket(new RemoteChatSession.Data(session, data)));
      var data2 = new net.minecraft.world.entity.player.ProfilePublicKey.Data(java.time.Instant.ofEpochMilli(-5L), kp.getPublic(), new byte[0]);
      PlayHarness.sb("chat_session_update/empty_signature", new ServerboundChatSessionUpdatePacket(new RemoteChatSession.Data(new UUID(0, 0), data2)));
      var data3 = new net.minecraft.world.entity.player.ProfilePublicKey.Data(java.time.Instant.ofEpochMilli(0L), kp.getPublic(), new byte[4096]);
      PlayHarness.sb("chat_session_update/max_signature", new ServerboundChatSessionUpdatePacket(new RemoteChatSession.Data(session, data3)));
      err("chat_session_update/signature_4097", ServerboundChatSessionUpdatePacket.STREAM_CODEC, bytes(b -> {
         b.writeUUID(session); b.writeLong(0); b.writeByteArray(der); b.writeByteArray(new byte[4097]); }));
      err("chat_session_update/key_513", ServerboundChatSessionUpdatePacket.STREAM_CODEC, bytes(b -> {
         b.writeUUID(session); b.writeLong(0); b.writeByteArray(new byte[513]); b.writeByteArray(new byte[0]); }));
      err("chat_session_update/key_garbage", ServerboundChatSessionUpdatePacket.STREAM_CODEC, bytes(b -> {
         b.writeUUID(session); b.writeLong(0); b.writeByteArray(new byte[]{1, 2, 3}); b.writeByteArray(new byte[0]); }));

      // command suggestion: both bindings encode alike
      for (boolean cb : new boolean[]{true, false}) {
         PlayHarness.commandBlocks = cb;
         PlayHarness.rebindServerbound();
         String p = cb ? "command_suggestion/blocks_" : "command_suggestion/chat_";
         PlayHarness.sb(p + "basic", new ServerboundCommandSuggestionPacket(3, "/say h"));
         PlayHarness.sb(p + "empty", new ServerboundCommandSuggestionPacket(0, ""));
         PlayHarness.sb(p + "unicode", new ServerboundCommandSuggestionPacket(-1, "\uD83D\uDE00 caf\u00e9"));
         PlayHarness.sb(p + "len256", new ServerboundCommandSuggestionPacket(Integer.MAX_VALUE, rep("c", 256)));
      }
      PlayHarness.commandBlocks = true;
      PlayHarness.rebindServerbound();
      PlayHarness.sb("command_suggestion/len32500", new ServerboundCommandSuggestionPacket(1, rep("c", 32500)));
      PlayHarness.raw("command_suggestion/body_len257", b -> ServerboundCommandSuggestionPacket.STREAM_CODEC.encode(b, new ServerboundCommandSuggestionPacket(1, rep("c", 257))));
      PlayHarness.raw("command_suggestion/body_len257_utf16", b -> ServerboundCommandSuggestionPacket.STREAM_CODEC.encode(b, new ServerboundCommandSuggestionPacket(1, rep("\uD83D\uDE00", 129))));
      err("command_suggestion/blocks_32501", ServerboundCommandSuggestionPacket.STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf(rep("c", 32501), 40000); }));
      err("command_suggestion/chat_only_257", ServerboundCommandSuggestionPacket.CHAT_ONLY_STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf(rep("c", 257), 40000); }));
      err("command_suggestion/chat_only_257_utf16", ServerboundCommandSuggestionPacket.CHAT_ONLY_STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf(rep("\uD83D\uDE00", 129), 40000); }));
      err("command_suggestion/chat_only_32501", ServerboundCommandSuggestionPacket.CHAT_ONLY_STREAM_CODEC, bytes(b -> { b.writeVarInt(1); b.writeUtf(rep("c", 32501), 40000); }));
      err("command_suggestion/chat_only_bad_varint", ServerboundCommandSuggestionPacket.CHAT_ONLY_STREAM_CODEC, new byte[]{(byte)0x80, (byte)0x80, (byte)0x80, (byte)0x80, (byte)0x80, 1});
      // 257 chars are fine with the command-block codec and rejected with the chat-only one
      PlayHarness.raw("command_suggestion/ok_257_blocks", b -> ServerboundCommandSuggestionPacket.STREAM_CODEC.encode(b, new ServerboundCommandSuggestionPacket(1, rep("c", 257))));

      playerInfo(session, new RemoteChatSession.Data(session, data), new RemoteChatSession.Data(new UUID(5, -6), data3));
      commands();
      PlayHarness.close();
   }

   // ---- player info ----------------------------------------------------------------------------------------------

   static ClientboundPlayerInfoUpdatePacket info(EnumSet<ClientboundPlayerInfoUpdatePacket.Action> actions, List<ClientboundPlayerInfoUpdatePacket.Entry> entries) throws Exception {
      var p = new ClientboundPlayerInfoUpdatePacket(actions, List.of());
      Field f = ClientboundPlayerInfoUpdatePacket.class.getDeclaredField("entries");
      f.setAccessible(true);
      f.set(p, entries);
      return p;
   }

   static GameProfile profile(UUID id, String name, Property... props) {
      var b = ImmutableMultimap.<String, Property>builder();
      for (Property p : props) b.put(p.name(), p);
      return new GameProfile(id, name, new PropertyMap(b.build()));
   }

   static void playerInfo(UUID session, RemoteChatSession.Data d1, RemoteChatSession.Data d2) throws Exception {
      var A = ClientboundPlayerInfoUpdatePacket.Action.values();
      UUID u1 = UUID.fromString("11111111-2222-3333-4444-555555555555"), u2 = new UUID(-1L, 0L), u3 = new UUID(0L, Long.MIN_VALUE);
      var e1 = new ClientboundPlayerInfoUpdatePacket.Entry(u1, profile(u1, "Steve", new Property("textures", "dGV4dHVyZXM=", "c2ln"), new Property("plain", "v")),
         true, 42, GameType.CREATIVE, COMPS[4], true, 3, d1);
      var e2 = new ClientboundPlayerInfoUpdatePacket.Entry(u2, profile(u2, "A\uD83D\uDE00c"), false, 0, GameType.ADVENTURE, null, false, 0, null);
      var e3 = new ClientboundPlayerInfoUpdatePacket.Entry(u3, profile(u3, "Sixteen_chars_ok"), true, Integer.MAX_VALUE, GameType.SPECTATOR, COMPS[2], true, -1, d2);
      for (int mask = 0; mask < 256; mask++) {
         var set = EnumSet.noneOf(ClientboundPlayerInfoUpdatePacket.Action.class);
         for (int i = 0; i < 8; i++) if ((mask >> i & 1) != 0) set.add(A[i]);
         PlayHarness.cb("player_info_update/mask_" + mask, info(set, List.of(e1, e2, e3)));
      }
      var all = EnumSet.allOf(ClientboundPlayerInfoUpdatePacket.Action.class);
      PlayHarness.cb("player_info_update/none_entries", info(all, List.of()));
      PlayHarness.cb("player_info_update/one_entry", info(all, List.of(e2)));
      PlayHarness.cb("player_info_update/latency_negative", info(EnumSet.of(A[4], A[6]), List.of(new ClientboundPlayerInfoUpdatePacket.Entry(u1, null, false, -1, GameType.SURVIVAL, null, false, Integer.MIN_VALUE, null))));
      var props16 = new Property[16];
      for (int i = 0; i < 16; i++) props16[i] = new Property("p" + i, rep("v", 100 * i), i % 2 == 0 ? null : rep("s", 1024));
      var big = new ClientboundPlayerInfoUpdatePacket.Entry(u1, profile(u1, "Steve", props16), true, 1, GameType.SURVIVAL, null, false, 0, null);
      PlayHarness.cb("player_info_update/sixteen_properties", info(EnumSet.of(A[0]), List.of(big)));
      var longName = profile(u1, rep("\uD83D\uDE00", 8));
      PlayHarness.cb("player_info_update/name_16_units_non_bmp", info(EnumSet.of(A[0]), List.of(new ClientboundPlayerInfoUpdatePacket.Entry(u1, longName, false, 0, GameType.SURVIVAL, null, false, 0, null))));
      var many = new ArrayList<ClientboundPlayerInfoUpdatePacket.Entry>();
      for (int i = 0; i < 100; i++) many.add(new ClientboundPlayerInfoUpdatePacket.Entry(new UUID(i, ~i), profile(new UUID(i, ~i), "P" + i), i % 2 == 0, i * 7, GameType.byId(i % 4), i % 3 == 0 ? Component.literal("n" + i) : null, i % 5 == 0, i, null));
      PlayHarness.cb("player_info_update/hundred", info(ClientboundPlayerInfoUpdatePacket.Action.values().length == 8 ? EnumSet.of(A[0], A[2], A[3], A[4], A[5], A[6], A[7]) : all, many));

      PlayHarness.cb("player_info_remove/empty", new ClientboundPlayerInfoRemovePacket(List.of()));
      PlayHarness.cb("player_info_remove/one", new ClientboundPlayerInfoRemovePacket(List.of(u1)));
      PlayHarness.cb("player_info_remove/three", new ClientboundPlayerInfoRemovePacket(List.of(u1, u2, u3)));

      var STREAM = ClientboundPlayerInfoUpdatePacket.STREAM_CODEC;
      java.util.function.BiFunction<Integer, Consumer<RegistryFriendlyByteBuf>, byte[]> addPlayer = (n, w) -> bytes(b -> { b.writeByte(1); b.writeVarInt(1); b.writeUUID(u1); w.accept(b); });
      err("player_info_update/name_17", STREAM, addPlayer.apply(0, b -> { b.writeUtf(rep("n", 17), 100); b.writeVarInt(0); }));
      err("player_info_update/name_17_utf16", STREAM, addPlayer.apply(0, b -> { b.writeUtf(rep("\uD83D\uDE00", 9), 100); b.writeVarInt(0); }));
      err("player_info_update/properties_17", STREAM, addPlayer.apply(0, b -> { b.writeUtf("n", 100); b.writeVarInt(17); }));
      err("player_info_update/property_name_65", STREAM, addPlayer.apply(0, b -> { b.writeUtf("n", 100); b.writeVarInt(1); b.writeUtf(rep("p", 65), 100); }));
      err("player_info_update/property_value_32768", STREAM, addPlayer.apply(0, b -> { b.writeUtf("n", 100); b.writeVarInt(1); b.writeUtf("p"); b.writeUtf(rep("v", 32768), 40000); }));
      err("player_info_update/property_signature_1025", STREAM, addPlayer.apply(0, b -> { b.writeUtf("n", 100); b.writeVarInt(1); b.writeUtf("p"); b.writeUtf("v"); b.writeBoolean(true); b.writeUtf(rep("s", 1025), 2000); }));
      err("player_info_update/chat_session_bad_key", STREAM, bytes(b -> { b.writeByte(2); b.writeVarInt(1); b.writeUUID(u1); b.writeBoolean(true); b.writeUUID(session); b.writeLong(0); b.writeByteArray(new byte[]{9}); b.writeByteArray(new byte[0]); }));
      lossy("player_info_update/unknown_game_mode", STREAM, bytes(b -> { b.writeByte(4); b.writeVarInt(1); b.writeUUID(u1); b.writeVarInt(9); }));
      lossy("player_info_update/negative_game_mode", STREAM, bytes(b -> { b.writeByte(4); b.writeVarInt(1); b.writeUUID(u1); b.writeVarInt(-3); }));
      err("player_info_update/truncated", STREAM, bytes(b -> { b.writeByte(0x10); b.writeVarInt(1); b.writeUUID(u1); }));
   }

   // ---- commands -------------------------------------------------------------------------------------------------

   static ClientboundCommandsPacket cmds(RootCommandNode<Object> root, Map<String, Identifier> sugg, Set<String> restricted) {
      return new ClientboundCommandsPacket(root, new ClientboundCommandsPacket.NodeInspector<Object>() {
         public Identifier suggestionId(ArgumentCommandNode<Object, ?> n) { return sugg.get(n.getName()); }
         public boolean isExecutable(CommandNode<Object> n) { return n.getCommand() != null; }
         public boolean isRestricted(CommandNode<Object> n) { return restricted.contains(n.getName()); }
      });
   }

   static LiteralArgumentBuilder<Object> lit(String n) { return LiteralArgumentBuilder.literal(n); }

   static <T> RequiredArgumentBuilder<Object, T> arg(String n, ArgumentType<T> t) { return RequiredArgumentBuilder.argument(n, t); }

   static final Command<Object> RUN = c -> 1;

   @SuppressWarnings({"rawtypes", "unchecked"})
   static void commands() throws Exception {
      CommandBuildContext ctx = Commands.createValidationContext(PlayHarness.access);
      var A = new LinkedHashMap<String, ArgumentType<?>>();
      A.put("bool", BoolArgumentType.bool());
      A.put("float_unbounded", FloatArgumentType.floatArg());
      A.put("float_min", FloatArgumentType.floatArg(-1.5f));
      A.put("float_both", FloatArgumentType.floatArg(-1.5f, 2.5f));
      A.put("float_max_only", FloatArgumentType.floatArg(-Float.MAX_VALUE, 3.0f));
      A.put("float_nan_edge", FloatArgumentType.floatArg(Float.MIN_VALUE, Float.POSITIVE_INFINITY));
      A.put("double_unbounded", DoubleArgumentType.doubleArg());
      A.put("double_min", DoubleArgumentType.doubleArg(-3e10));
      A.put("double_both", DoubleArgumentType.doubleArg(0.1, 1e300));
      A.put("double_max_only", DoubleArgumentType.doubleArg(-Double.MAX_VALUE, -0.5));
      A.put("integer_unbounded", IntegerArgumentType.integer());
      A.put("integer_min", IntegerArgumentType.integer(-5));
      A.put("integer_both", IntegerArgumentType.integer(-2147483647, 2147483646));
      A.put("integer_max_only", IntegerArgumentType.integer(Integer.MIN_VALUE, 100));
      A.put("long_unbounded", LongArgumentType.longArg());
      A.put("long_min", LongArgumentType.longArg(-9000000000L));
      A.put("long_both", LongArgumentType.longArg(1L, Long.MAX_VALUE - 1));
      A.put("long_max_only", LongArgumentType.longArg(Long.MIN_VALUE, 77L));
      A.put("string_word", StringArgumentType.word());
      A.put("string_phrase", StringArgumentType.string());
      A.put("string_greedy", StringArgumentType.greedyString());
      A.put("entity_entity", EntityArgument.entity());
      A.put("entity_entities", EntityArgument.entities());
      A.put("entity_player", EntityArgument.player());
      A.put("entity_players", EntityArgument.players());
      A.put("game_profile", GameProfileArgument.gameProfile());
      A.put("block_pos", BlockPosArgument.blockPos());
      A.put("column_pos", ColumnPosArgument.columnPos());
      A.put("vec3", Vec3Argument.vec3());
      A.put("vec2", Vec2Argument.vec2());
      A.put("block_state", BlockStateArgument.block(ctx));
      A.put("block_predicate", BlockPredicateArgument.blockPredicate(ctx));
      A.put("item_stack", ItemArgument.item(ctx));
      A.put("item_predicate", ItemPredicateArgument.itemPredicate(ctx));
      A.put("team_color", TeamColorArgument.teamColor());
      A.put("hex_color", HexColorArgument.hexColor());
      A.put("component", ComponentArgument.textComponent(ctx));
      A.put("style", StyleArgument.style(ctx));
      A.put("message", MessageArgument.message());
      A.put("nbt_compound_tag", CompoundTagArgument.compoundTag());
      A.put("nbt_tag", NbtTagArgument.nbtTag());
      A.put("nbt_path", NbtPathArgument.nbtPath());
      A.put("objective", ObjectiveArgument.objective());
      A.put("objective_criteria", ObjectiveCriteriaArgument.criteria());
      A.put("operation", OperationArgument.operation());
      A.put("particle", ParticleArgument.particle(ctx));
      A.put("angle", AngleArgument.angle());
      A.put("rotation", RotationArgument.rotation());
      A.put("scoreboard_slot", ScoreboardSlotArgument.displaySlot());
      A.put("score_holder_single", ScoreHolderArgument.scoreHolder());
      A.put("score_holder_multiple", ScoreHolderArgument.scoreHolders());
      A.put("swizzle", SwizzleArgument.swizzle());
      A.put("team", TeamArgument.team());
      A.put("item_slot", SlotArgument.slot());
      A.put("item_slots", SlotsArgument.slots());
      A.put("resource_location", IdentifierArgument.id());
      A.put("function", FunctionArgument.functions());
      A.put("entity_anchor", EntityAnchorArgument.anchor());
      A.put("int_range", RangeArgument.intRange());
      A.put("float_range", RangeArgument.floatRange());
      A.put("dimension", DimensionArgument.dimension());
      A.put("gamemode", GameModeArgument.gameMode());
      A.put("time_default", TimeArgument.time());
      A.put("time_min", TimeArgument.time(100));
      A.put("time_negative", TimeArgument.time(Integer.MIN_VALUE));
      A.put("resource_or_tag_item", ResourceOrTagArgument.resourceOrTag(ctx, Registries.ITEM));
      A.put("resource_or_tag_biome", ResourceOrTagArgument.resourceOrTag(ctx, Registries.BIOME));
      A.put("resource_or_tag_key_block", ResourceOrTagKeyArgument.resourceOrTagKey(Registries.BLOCK));
      A.put("resource_or_tag_key_structure", ResourceOrTagKeyArgument.resourceOrTagKey(Registries.STRUCTURE));
      A.put("resource_enchantment", ResourceArgument.resource(ctx, Registries.ENCHANTMENT));
      A.put("resource_entity_type", ResourceArgument.resource(ctx, Registries.ENTITY_TYPE));
      A.put("resource_key_advancement", ResourceKeyArgument.key(Registries.ADVANCEMENT));
      A.put("resource_key_recipe", ResourceKeyArgument.key(Registries.RECIPE));
      A.put("resource_selector_biome", ResourceSelectorArgument.resourceSelector(ctx, Registries.BIOME));
      A.put("resource_selector_item", ResourceSelectorArgument.resourceSelector(ctx, Registries.ITEM));
      A.put("template_mirror", TemplateMirrorArgument.templateMirror());
      A.put("template_rotation", TemplateRotationArgument.templateRotation());
      A.put("heightmap", HeightmapTypeArgument.heightmap());
      A.put("loot_table", ResourceOrIdArgument.lootTable(ctx));
      A.put("loot_predicate", ResourceOrIdArgument.lootPredicate(ctx));
      A.put("loot_modifier", ResourceOrIdArgument.lootModifier(ctx));
      A.put("context_float_provider", ResourceOrIdArgument.floatProvider(ctx));
      A.put("context_int_provider", ResourceOrIdArgument.intProvider(ctx));
      A.put("slot_source", SlotSourceArgument.slotSource(ctx));
      A.put("dialog", ResourceOrIdArgument.dialog(ctx));
      A.put("feature", ResourceOrIdArgument.feature(ctx));
      A.put("swing_animation", SwingAnimationArgument.swingAnimationType());
      A.put("uuid", UuidArgument.uuid());
      for (var e : A.entrySet()) {
         var root = new RootCommandNode<Object>();
         root.addChild(lit("t").then(arg("a", (ArgumentType)e.getValue())).build());
         PlayHarness.cb("commands/arg_" + e.getKey(), cmds(root, Map.of(), Set.of()));
      }
      // suggestion providers + flags on one argument node
      for (String s : new String[]{"minecraft:ask_server", "minecraft:available_sounds", "minecraft:summonable_entities", "minecraft:all_recipes", "mod:custom/provider"}) {
         var root = new RootCommandNode<Object>();
         root.addChild(lit("t").then(arg("a", StringArgumentType.word()).executes(RUN)).build());
         PlayHarness.cb("commands/suggest_" + s.replace(':', '_').replace('/', '_'), cmds(root, Map.of("a", id(s)), Set.of()));
      }

      // root only
      PlayHarness.cb("commands/root_only", cmds(new RootCommandNode<>(), Map.of(), Set.of()));
      // flag combinations on a literal and an argument
      for (int f = 0; f < 4; f++) {
         var root = new RootCommandNode<Object>();
         var l = lit("lit");
         var r = arg("arg", IntegerArgumentType.integer(0));
         if ((f & 1) != 0) { l.executes(RUN); r.executes(RUN); }
         root.addChild(l.then(r).build());
         PlayHarness.cb("commands/flags_" + f, cmds(root, (f & 2) != 0 ? Map.of("arg", id("minecraft:ask_server")) : Map.of(), (f & 2) != 0 ? Set.of("lit", "arg") : Set.of()));
      }
      // a vanilla-like tree: shared nodes, redirects to root and to an inner node, restricted literals
      var root = new RootCommandNode<Object>();
      LiteralCommandNode<Object> execute = lit("execute").build();
      execute.addChild(lit("run").redirect(root).build());
      execute.addChild(lit("as").then(arg("targets", EntityArgument.entities()).redirect(execute)).build());
      execute.addChild(lit("if").then(lit("block").then(arg("pos", BlockPosArgument.blockPos()).then(arg("block", BlockPredicateArgument.blockPredicate(ctx)).executes(RUN).redirect(execute)))).build());
      root.addChild(execute);
      root.addChild(lit("say").then(arg("message", MessageArgument.message()).executes(RUN)).build());
      root.addChild(lit("stop").executes(RUN).build());
      root.addChild(lit("give").then(arg("targets", EntityArgument.players()).then(arg("item", ItemArgument.item(ctx)).executes(RUN).then(arg("count", IntegerArgumentType.integer(1)).executes(RUN)))).build());
      root.addChild(lit("tp").then(arg("destination", EntityArgument.entity()).executes(RUN)).then(arg("location", Vec3Argument.vec3()).executes(RUN)).build());
      root.addChild(lit("playsound").then(arg("sound", IdentifierArgument.id())).build());
      PlayHarness.cb("commands/tree", cmds(root, Map.of("sound", id("minecraft:available_sounds")), Set.of("stop", "give")));
      // many nodes
      var big = new RootCommandNode<Object>();
      for (int i = 0; i < 300; i++) big.addChild(lit("cmd" + i).executes(RUN).then(arg("x", DoubleArgumentType.doubleArg(i))).build());
      PlayHarness.cb("commands/many", cmds(big, Map.of(), Set.of()));

      // ---- crafted node lists: tree validation and vanilla's lenient reads --------------------------------------
      java.util.function.Function<List<Consumer<RegistryFriendlyByteBuf>>, byte[]> list = nodes -> bytes(b -> { b.writeVarInt(nodes.size()); nodes.forEach(n -> n.accept(b)); b.writeVarInt(0); });
      Consumer<RegistryFriendlyByteBuf> root1 = b -> { b.writeByte(0); b.writeVarIntArray(new int[]{1}); };
      Consumer<RegistryFriendlyByteBuf> leaf = b -> { b.writeByte(1 | 4); b.writeVarIntArray(new int[0]); b.writeUtf("leaf"); };
      var C = ClientboundCommandsPacket.STREAM_CODEC;
      lossy("commands_bad/valid_two_nodes", C, list.apply(List.of(root1, leaf)));
      // node 1 redirects to itself
      err("commands_bad/self_redirect", C, list.apply(List.of(root1, b -> { b.writeByte(1 | 8); b.writeVarIntArray(new int[0]); b.writeVarInt(1); b.writeUtf("x"); })));
      // 1 -> 2 -> 1 redirects
      err("commands_bad/redirect_cycle", C, list.apply(List.of(root1,
         b -> { b.writeByte(1 | 8); b.writeVarIntArray(new int[0]); b.writeVarInt(2); b.writeUtf("x"); },
         b -> { b.writeByte(1 | 8); b.writeVarIntArray(new int[0]); b.writeVarInt(1); b.writeUtf("y"); })));
      // root is its own child
      err("commands_bad/self_child", C, list.apply(List.of(b -> { b.writeByte(0); b.writeVarIntArray(new int[]{0}); })));
      // 1 -> child 2 -> child 1
      err("commands_bad/child_cycle", C, list.apply(List.of(root1,
         b -> { b.writeByte(1); b.writeVarIntArray(new int[]{2}); b.writeUtf("x"); },
         b -> { b.writeByte(1); b.writeVarIntArray(new int[]{1}); b.writeUtf("y"); })));
      // a child + redirect loop is not a cycle in either validation pass
      lossy("commands_bad/child_and_redirect_loop", C, list.apply(List.of(root1,
         b -> { b.writeByte(1); b.writeVarIntArray(new int[]{2}); b.writeUtf("x"); },
         b -> { b.writeByte(1 | 8); b.writeVarIntArray(new int[0]); b.writeVarInt(1); b.writeUtf("y"); })));
      // indices out of range are accepted when decoding
      lossy("commands_bad/out_of_range", C, list.apply(List.of(b -> { b.writeByte(1 | 8); b.writeVarIntArray(new int[]{5, -1, 99}); b.writeVarInt(77); b.writeUtf("x"); })));
      lossy("commands_bad/negative_redirect", C, list.apply(List.of(b -> { b.writeByte(1 | 8); b.writeVarIntArray(new int[0]); b.writeVarInt(-1); b.writeUtf("x"); })));
      // node type 3 and an unknown argument type id read as stub-less nodes
      lossy("commands_bad/type3", C, list.apply(List.of(b -> { b.writeByte(3 | 4); b.writeVarIntArray(new int[0]); })));
      lossy("commands_bad/unknown_argument_type", C, list.apply(List.of(b -> { b.writeByte(2 | 16); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(999); })));
      lossy("commands_bad/negative_argument_type", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(-1); })));
      // flag 16 without an argument node is ignored, bits >= 64 are ignored
      lossy("commands_bad/stray_flags", C, list.apply(List.of(b -> { b.writeByte(1 | 16 | 64 | 128 | 32); b.writeVarIntArray(new int[0]); b.writeUtf("a"); })));
      // string argument with an out of range type, bad identifier
      err("commands_bad/string_type_3", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(5); b.writeVarInt(3); })));
      err("commands_bad/string_type_negative", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(5); b.writeVarInt(-1); })));
      err("commands_bad/bad_suggestion_id", C, list.apply(List.of(b -> { b.writeByte(2 | 16); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(0); b.writeUtf("Not Valid"); })));
      err("commands_bad/bad_resource_registry", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(46); b.writeUtf("a:b:c"); })));
      // number flags with stray bits; bounds equal to the extremes
      lossy("commands_bad/number_flags", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(3); b.writeByte(0xfc); })));
      lossy("commands_bad/number_extreme_bounds", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(3); b.writeByte(3); b.writeInt(Integer.MIN_VALUE); b.writeInt(Integer.MAX_VALUE); })));
      lossy("commands_bad/entity_flags", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(6); b.writeByte(0xfc); })));
      lossy("commands_bad/score_holder_flags", C, list.apply(List.of(b -> { b.writeByte(2); b.writeVarIntArray(new int[0]); b.writeUtf("a"); b.writeVarInt(31); b.writeByte(0xff); })));
      // children array bigger than the bytes left; negative; truncated
      err("commands_bad/children_too_many", C, bytes(b -> { b.writeVarInt(1); b.writeByte(0); b.writeVarInt(100); b.writeVarInt(1); }));
      err("commands_bad/children_negative", C, bytes(b -> { b.writeVarInt(1); b.writeByte(0); b.writeVarInt(-1); b.writeVarInt(0); }));
      err("commands_bad/truncated", C, bytes(b -> { b.writeVarInt(2); b.writeByte(0); b.writeVarIntArray(new int[]{1}); }));
      err("commands_bad/negative_count", C, bytes(b -> b.writeVarInt(-1)));
      lossy("commands_bad/root_index_out_of_range", C, bytes(b -> { b.writeVarInt(0); b.writeVarInt(5); }));
   }
}
