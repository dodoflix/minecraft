import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import net.minecraft.network.chat.*;
import net.minecraft.world.entity.player.ProfilePublicKey;
import net.minecraft.util.Signer;
import java.io.*;
import java.lang.reflect.Method;
import java.security.*;
import java.time.Instant;
import java.util.*;

/**
 * Golden generator for server::chat (cache, last-seen window, signed chain, filter mask, chat type decoration).
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/chat_golden.jsonl crates/mc-game/tests/fixtures/ChatGolden.java crates/mc-game/tests/fixtures/chat_golden.jsonl
 * Output: crates/mc-game/tests/fixtures/chat_golden.jsonl
 * (the output goes to the file named by the argument: vanilla logs 'Received expired chat' warnings to stdout, the fixed 2023 timestamps are older than 5 minutes)
 */
public class ChatGolden {
    static final Gson GSON = new GsonBuilder().disableHtmlEscaping().create();

    static MessageSignature sig(int i) {
        byte[] b = new byte[256];
        Arrays.fill(b, (byte) 0x5a);
        b[0] = (byte) i;
        b[1] = (byte) (i >> 8);
        return new MessageSignature(b);
    }

    static int idOf(MessageSignature s) {
        return s == null ? -1 : (s.bytes()[0] & 0xff) | ((s.bytes()[1] & 0xff) << 8);
    }

    static JsonArray ids(Collection<MessageSignature> l) {
        JsonArray a = new JsonArray();
        for (MessageSignature s : l) a.add(idOf(s));
        return a;
    }

    static String hex(byte[] b) {
        StringBuilder sb = new StringBuilder();
        for (byte x : b) sb.append(String.format(java.util.Locale.ROOT, "%02x", x));
        return sb.toString();
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        PrintStream out = new PrintStream(new FileOutputStream(args[0]), true, "UTF-8");
        Random rnd = new Random(42);
        cache(out, rnd);
        lastSeen(out, rnd);
        chain(out);
        filter(out);
        chatTypes(out);
    }

    static void cache(PrintStream out, Random rnd) throws Exception {
        Method push = MessageSignatureCache.class.getDeclaredMethod("push", List.class);
        push.setAccessible(true);
        for (int cap : new int[]{4, 16, 128}) {
            MessageSignatureCache cache = new MessageSignatureCache(cap);
            JsonObject r = new JsonObject();
            r.addProperty("t", "cache");
            r.addProperty("capacity", cap);
            JsonArray ops = new JsonArray(), states = new JsonArray(), packs = new JsonArray();
            for (int n = 0; n < 60; n++) {
                int len = rnd.nextInt(7);
                List<MessageSignature> l = new ArrayList<>();
                JsonArray opIds = new JsonArray();
                for (int i = 0; i < len; i++) {
                    int id = rnd.nextInt(cap * 2 + 3);
                    l.add(sig(id));
                    opIds.add(id);
                }
                push.invoke(cache, l);
                ops.add(opIds);
                JsonArray st = new JsonArray();
                for (int i = 0; i < cap; i++) st.add(idOf(cache.unpack(i)));
                states.add(st);
                JsonArray pk = new JsonArray();
                for (int i = 0; i < cap * 2 + 3; i++) pk.add(cache.pack(sig(i)));
                packs.add(pk);
            }
            r.add("ops", ops);
            r.add("states", states);
            r.add("packs", packs);
            out.println(GSON.toJson(r));
        }
    }

    static void lastSeen(PrintStream out, Random rnd) {
        for (int scenario = 0; scenario < 40; scenario++) {
            LastSeenMessagesTracker tracker = new LastSeenMessagesTracker(20);
            LastSeenMessagesValidator validator = new LastSeenMessagesValidator(20);
            JsonObject r = new JsonObject();
            r.addProperty("t", "lastseen");
            JsonArray steps = new JsonArray();
            int next = 0;
            int corruptRate = scenario % 4; // 0: never corrupt
            for (int n = 0; n < 80; n++) {
                JsonObject s = new JsonObject();
                int kind = rnd.nextInt(10);
                if (kind == 9 && corruptRate == 0) kind = 7;
                if (kind < 6) {
                    // a message is sent; in the server's window it is pending; the client may or may not show it
                    boolean dup = rnd.nextInt(8) == 0 && next > 0;
                    int id = dup ? next - 1 : next++;
                    boolean shown = rnd.nextInt(5) != 0;
                    MessageSignature m = sig(id);
                    validator.addPending(m);
                    boolean added = tracker.addPending(m, shown);
                    s.addProperty("k", "msg");
                    s.addProperty("id", id);
                    s.addProperty("shown", shown);
                    s.addProperty("added", added);
                    s.addProperty("tracked", validator.trackedMessagesCount());
                    s.addProperty("offset", tracker.offset());
                } else if (kind < 9) {
                    LastSeenMessagesTracker.Update u = tracker.generateAndApplyUpdate();
                    int offset = u.update().offset();
                    BitSet ack = (BitSet) u.update().acknowledged().clone();
                    byte checksum = u.update().checksum();
                    s.addProperty("k", "update");
                    s.addProperty("genOffset", offset);
                    s.addProperty("genAck", ack.isEmpty() ? 0L : ack.toLongArray()[0]);
                    s.addProperty("genChecksum", checksum);
                    s.add("genLastSeen", ids(u.lastSeen().entries()));
                    if (corruptRate != 0 && rnd.nextInt(4) == 0) {
                        switch (rnd.nextInt(4)) {
                            case 0 -> offset += 1 + rnd.nextInt(3);
                            case 1 -> ack.flip(rnd.nextInt(20));
                            case 2 -> ack.set(20 + rnd.nextInt(6));
                            default -> checksum = (byte) (checksum + 1 + rnd.nextInt(5));
                        }
                    }
                    s.addProperty("offset", offset);
                    s.addProperty("ack", ack.isEmpty() ? 0L : ack.toLongArray()[0]);
                    s.addProperty("checksum", checksum);
                    try {
                        LastSeenMessages ls = validator.applyUpdate(new LastSeenMessages.Update(offset, ack, checksum));
                        s.add("ok", ids(ls.entries()));
                    } catch (LastSeenMessagesValidator.ValidationException e) {
                        s.addProperty("err", e.getMessage());
                    }
                } else {
                    int off = rnd.nextInt(5) - 1;
                    s.addProperty("k", "ackoffset");
                    s.addProperty("offset", off);
                    try {
                        validator.applyOffset(off);
                        s.addProperty("ok", true);
                    } catch (LastSeenMessagesValidator.ValidationException e) {
                        s.addProperty("err", e.getMessage());
                    }
                }
                s.addProperty("count", validator.trackedMessagesCount());
                steps.add(s);
            }
            r.add("steps", steps);
            out.println(GSON.toJson(r));
        }
    }

    static String errKey(SignedMessageChain.DecodeException e) {
        return ((net.minecraft.network.chat.contents.TranslatableContents) e.getComponent().getContents()).getKey();
    }

    static void chain(PrintStream out) throws Exception {
        KeyPairGenerator gen = KeyPairGenerator.getInstance("RSA");
        gen.initialize(2048);
        KeyPair kp = gen.generateKeyPair();
        UUID profile = UUID.fromString("11111111-2222-3333-4444-555555555555");
        UUID session = UUID.fromString("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        for (int scenario = 0; scenario < 6; scenario++) {
            boolean expired = scenario == 5;
            Instant expires = Instant.ofEpochSecond(expired ? 1_000_000_000L : 4_102_444_800L);
            ProfilePublicKey.Data data = new ProfilePublicKey.Data(expires, kp.getPublic(), new byte[0]);
            ProfilePublicKey key = new ProfilePublicKey(data);
            SignedMessageChain client = new SignedMessageChain(profile, session);
            SignedMessageChain server = new SignedMessageChain(profile, session);
            final byte[][] lastPayload = new byte[1][];
            Signer base = Signer.from(kp.getPrivate(), "SHA256withRSA");
            Signer signer = updater -> {
                ByteArrayOutputStream b = new ByteArrayOutputStream();
                try { updater.update(b::writeBytes); } catch (Exception e) { throw new RuntimeException(e); }
                lastPayload[0] = b.toByteArray();
                return base.sign(lastPayload[0]);
            };
            SignedMessageChain.Encoder enc = client.encoder(signer);
            SignedMessageChain.Decoder dec = server.decoder(key);
            JsonObject r = new JsonObject();
            r.addProperty("t", "chain");
            r.addProperty("pubkey", hex(kp.getPublic().getEncoded()));
            r.addProperty("expires_at", expires.toEpochMilli());
            r.addProperty("profile", profile.toString());
            r.addProperty("session", session.toString());
            JsonArray msgs = new JsonArray();
            long ts = 1_700_000_000_000L;
            Random rnd = new Random(scenario);
            for (int i = 0; i < 6; i++) {
                ts += scenario == 3 && i == 3 ? -5000 : 1000 + rnd.nextInt(500);
                String content = i == 2 ? "héllo wörld 😀 §" : "message " + i;
                int nl = rnd.nextInt(4);
                List<MessageSignature> seen = new ArrayList<>();
                for (int k = 0; k < nl; k++) seen.add(sig(100 + rnd.nextInt(30)));
                SignedMessageBody body = new SignedMessageBody(content, Instant.ofEpochMilli(ts), rnd.nextLong(), new LastSeenMessages(seen));
                MessageSignature signature = enc.pack(body);
                JsonObject m = new JsonObject();
                m.addProperty("content", content);
                m.addProperty("ts", ts);
                m.addProperty("salt", body.salt());
                m.add("seen", ids(seen));
                m.addProperty("signature", hex(signature.bytes()));
                m.addProperty("payload", hex(lastPayload[0]));
                // tamper variants per scenario
                MessageSignature delivered = signature;
                SignedMessageBody deliveredBody = body;
                if (scenario == 1 && i == 2) { byte[] b = signature.bytes().clone(); b[10] ^= 1; delivered = new MessageSignature(b); m.addProperty("tamper", "flipsig"); }
                if (scenario == 2 && i == 1) { delivered = null; m.addProperty("tamper", "nosig"); }
                if (scenario == 4 && i == 4) { deliveredBody = new SignedMessageBody(content + "!", body.timeStamp(), body.salt(), body.lastSeen()); m.addProperty("tamper", "content"); }
                if (scenario == 4 && i == 1) { m.addProperty("skip", true); msgs.add(m); continue; }
                try {
                    PlayerChatMessage pm = dec.unpack(delivered, deliveredBody);
                    m.addProperty("ok", true);
                    m.addProperty("link_index", pm.link().index());
                } catch (SignedMessageChain.DecodeException e) {
                    m.addProperty("err", errKey(e));
                }
                msgs.add(m);
            }
            r.add("messages", msgs);
            out.println(GSON.toJson(r));
        }
    }

    static void filter(PrintStream out) {
        String[] texts = {"hello world", "", "a", "😀 emoji😀 text", "naughty words here"};
        for (String text : texts) {
            for (int pat = 0; pat < 5; pat++) {
                FilterMask mask;
                JsonObject r = new JsonObject();
                r.addProperty("t", "filter");
                r.addProperty("text", text);
                JsonArray bits = new JsonArray();
                if (pat == 0) { mask = FilterMask.PASS_THROUGH; r.addProperty("kind", "pass"); }
                else if (pat == 1) { mask = FilterMask.FULLY_FILTERED; r.addProperty("kind", "full"); }
                else {
                    r.addProperty("kind", "partial");
                    mask = new FilterMask(text.length());
                    int len = text.length();
                    for (int i = 0; i < len; i++) {
                        boolean set = switch (pat) { case 2 -> i % 2 == 0; case 3 -> i >= len / 2; default -> i > 0 && i < 5; };
                        if (set) { mask.setFiltered(i); bits.add(i); }
                    }
                }
                r.add("bits", bits);
                String applied = mask.apply(text);
                r.addProperty("applied", applied);
                r.addProperty("applied_null", applied == null);
                Component c = mask.applyWithFormatting(text);
                r.add("formatted", c == null ? JsonNull.INSTANCE : ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, c).getOrThrow());
                out.println(GSON.toJson(r));
            }
        }
    }

    static void chatTypes(PrintStream out) {
        // ChatType.bootstrap into a recording context is awkward; decorations are applied through the public records
        ChatTypeDecoration[] decorations = {
            ChatTypeDecoration.withSender("chat.type.text"),
            ChatTypeDecoration.incomingDirectMessage("commands.message.display.incoming"),
            ChatTypeDecoration.outgoingDirectMessage("commands.message.display.outgoing"),
            ChatTypeDecoration.teamMessage("chat.type.team.text"),
        };
        for (ChatTypeDecoration d : decorations) {
            ChatType type = new ChatType(d, d);
            net.minecraft.core.Holder<ChatType> holder = net.minecraft.core.Holder.direct(type);
            ChatType.Bound bound = new ChatType.Bound(holder, Component.literal("Steve"), Optional.of(Component.literal("Alex")));
            JsonObject r = new JsonObject();
            r.addProperty("t", "decorate");
            r.addProperty("key", d.translationKey());
            r.add("chat", ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, bound.decorate(Component.literal("hi there"))).getOrThrow());
            r.addProperty("string", bound.decorate(Component.literal("hi there")).getString());
            out.println(GSON.toJson(r));
        }
    }
}
