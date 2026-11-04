// Generates net_framing.txt: run through the real vanilla pipeline handlers (EmbeddedChannel).
//   scripts/vanilla-java.sh crates/mc-protocol/tests/fixtures/NetFraming.java > crates/mc-protocol/tests/fixtures/net_framing.txt
// Line format: <scenario> <threshold> <secret-hex or -> <payload-size> <wire-hex>; payload(n) = [0x2b, ((i*7 + i/13) % 251) for i in 1..n]
// Packets of one scenario are sent through ONE stream (cipher state continues across lines).
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import io.netty.channel.embedded.EmbeddedChannel;
import net.minecraft.network.*;
import net.minecraft.util.Crypt;
import javax.crypto.Cipher;
import javax.crypto.SecretKey;
import javax.crypto.spec.SecretKeySpec;

public class NetFraming {
    static byte[] payload(int n) {
        byte[] b = new byte[n];
        b[0] = 0x2b; // a packet id
        for (int i = 1; i < n; i++) b[i] = (byte) ((i * 7 + i / 13) % 251);
        return b;
    }

    public static void main(String[] a) throws Exception {
        int[] sizes = {1, 6, 255, 256, 257, 5000, 17000};
        String[] scen = {"plain", "comp", "enc", "both"};
        for (String s : scen) {
            int threshold = s.equals("comp") || s.equals("both") ? 256 : -1;
            boolean enc = s.equals("enc") || s.equals("both");
            byte[] secret = new byte[16];
            for (int i = 0; i < 16; i++) secret[i] = (byte) (0xA0 + i * 3);
            SecretKey key = new SecretKeySpec(secret, "AES");
            // outbound order (tail -> head): compress, prepender, encrypt
            java.util.List<io.netty.channel.ChannelHandler> out = new java.util.ArrayList<>();
            java.util.List<io.netty.channel.ChannelHandler> in = new java.util.ArrayList<>();
            if (enc) {
                out.add(new CipherEncoder(Crypt.getCipher(Cipher.ENCRYPT_MODE, key)));
                in.add(new CipherDecoder(Crypt.getCipher(Cipher.DECRYPT_MODE, key)));
            }
            out.add(new Varint21LengthFieldPrepender());
            in.add(new Varint21FrameDecoder(null));
            if (threshold >= 0) {
                out.add(new CompressionEncoder(threshold));
                in.add(new CompressionDecoder(threshold, true));
            }
            EmbeddedChannel enc_ch = new EmbeddedChannel(out.toArray(new io.netty.channel.ChannelHandler[0]));
            EmbeddedChannel dec_ch = new EmbeddedChannel(in.toArray(new io.netty.channel.ChannelHandler[0]));
            for (int n : sizes) {
                byte[] p = payload(n);
                enc_ch.writeOutbound(Unpooled.wrappedBuffer(p));
                ByteBuf wire = enc_ch.readOutbound();
                byte[] w = ByteBufUtil.getBytes(wire);
                dec_ch.writeInbound(Unpooled.wrappedBuffer(w.clone()));
                ByteBuf back = dec_ch.readInbound();
                if (!java.util.Arrays.equals(ByteBufUtil.getBytes(back), p)) throw new AssertionError("vanilla roundtrip " + s + " " + n);
                System.out.println(s + " " + threshold + " " + (enc ? ByteBufUtil.hexDump(secret) : "-") + " "
                    + n + " " + ByteBufUtil.hexDump(w));
            }
        }
    }
}
