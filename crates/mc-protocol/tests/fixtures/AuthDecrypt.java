// Decrypts with vanilla Crypt.decryptUsingKey; prints plaintext hex (ad-hoc query tool, no fixture is written).
// Usage: scripts/vanilla-java.sh crates/mc-protocol/tests/fixtures/AuthDecrypt.java <private_pkcs8_hex> <ciphertext_hex>
// Output: stdout only
import java.security.PrivateKey;
import java.security.KeyFactory;
import java.security.spec.PKCS8EncodedKeySpec;
import java.util.HexFormat;
import net.minecraft.util.Crypt;

public class AuthDecrypt {
   public static void main(String[] args) throws Exception {
      HexFormat hex = HexFormat.of();
      PrivateKey key = KeyFactory.getInstance("RSA").generatePrivate(new PKCS8EncodedKeySpec(hex.parseHex(args[0])));
      System.out.println(hex.formatHex(Crypt.decryptUsingKey(key, hex.parseHex(args[1]))));
   }
}
