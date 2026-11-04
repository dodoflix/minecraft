// Generates auth_golden.txt (key=hex lines) for crates/mc-protocol/tests/auth.rs:
//   scripts/vanilla-java.sh crates/mc-protocol/tests/fixtures/AuthGolden.java > crates/mc-protocol/tests/fixtures/auth_golden.txt
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.math.BigInteger;
import java.security.KeyPair;
import java.security.PublicKey;
import java.util.HexFormat;
import java.util.Random;
import javax.crypto.SecretKey;
import javax.crypto.spec.SecretKeySpec;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.util.Crypt;

public class AuthGolden {
   static final HexFormat HEX = HexFormat.of();

   static void p(String k, byte[] v) {
      System.out.println(k + "=" + HEX.formatHex(v));
   }

   // Crypt.digestData only calls getEncoded() on the public key, so any byte string can stand in for it.
   static PublicKey fakeKey(byte[] enc) {
      return new PublicKey() {
         public String getAlgorithm() { return "RSA"; }
         public String getFormat() { return "X.509"; }
         public byte[] getEncoded() { return enc; }
      };
   }

   public static void main(String[] args) throws Exception {
      KeyPair kp = Crypt.generateKeyPair();
      p("private_pkcs8", kp.getPrivate().getEncoded());
      p("public_der", kp.getPublic().getEncoded());

      // Encrypted by vanilla for the server key: a 16-byte secret, a 20-byte challenge, other secret lengths.
      byte[] secret = HEX.parseHex("00112233445566778899aabbccddeeff");
      p("secret", secret);
      p("secret_ct", Crypt.encryptUsingKey(kp.getPublic(), secret));
      byte[] challenge = HEX.parseHex("0102030405060708090a0b0c0d0e0f1011121314");
      p("challenge", challenge);
      p("challenge_ct", Crypt.encryptUsingKey(kp.getPublic(), challenge));
      p("secret24_ct", Crypt.encryptUsingKey(kp.getPublic(), new byte[24]));
      p("empty_ct", Crypt.encryptUsingKey(kp.getPublic(), new byte[0]));
      // Ciphertext with the leading zero byte stripped (Java accepts it as an integer).
      byte[] stripped = null;
      for (int i = 0; stripped == null; i++) {
         byte[] ct = Crypt.encryptUsingKey(kp.getPublic(), secret);
         if (ct[0] == 0) {
            stripped = java.util.Arrays.copyOfRange(ct, 1, ct.length);
         }
      }
      p("secret_ct_stripped", stripped);

      // ByteBufCodecs.PUBLIC_KEY wire bytes.
      ByteBuf buf = Unpooled.buffer();
      ByteBufCodecs.PUBLIC_KEY.encode(buf, kp.getPublic());
      byte[] wire = new byte[buf.readableBytes()];
      buf.readBytes(wire);
      p("public_wire", wire);

      // new BigInteger(Crypt.digestData(id, key, secret)).toString(16): "id|secret|key|expected".
      Random rnd = new Random(42);
      for (int i = 0; i < 200; i++) {
         String id = i % 10 == 1 ? "abc" : (i % 10 == 2 ? "\u00e9\u20ac\ud83d\ude00x" : "");
         byte[] s = new byte[i % 7 == 0 ? 1 + rnd.nextInt(40) : 16];
         rnd.nextBytes(s);
         byte[] k = i % 5 == 0 ? new byte[0] : (i % 5 == 1 ? kp.getPublic().getEncoded() : new byte[1 + rnd.nextInt(200)]);
         if (i % 5 > 1) rnd.nextBytes(k);
         SecretKey sk = new SecretKeySpec(s, "AES");
         String d = new BigInteger(Crypt.digestData(id, fakeKey(k), sk)).toString(16);
         System.out.println("digest=" + id + "|" + HEX.formatHex(s) + "|" + HEX.formatHex(k) + "|" + d);
      }
   }
}
