// Generates uuid.txt.
// Usage: scripts/vanilla-java.sh -o crates/mc-core/tests/fixtures/ident_uuid/uuid.txt crates/mc-core/tests/fixtures/ident_uuid/UuidGolden.java
// Output: crates/mc-core/tests/fixtures/ident_uuid/uuid.txt
// Names are escaped: every UTF-16 unit outside 0x20..0x7e and backslash becomes backslash-u plus 4 lowercase hex digits.
import com.mojang.util.UndashedUuid;
import java.util.*;
import net.minecraft.core.UUIDUtil;

public class UuidGolden {
   static String esc(String s) {
      StringBuilder b = new StringBuilder();
      for (char c : s.toCharArray()) {
         if (c < 0x20 || c > 0x7e || c == '\\') b.append(String.format(java.util.Locale.ROOT, "\\u%04x", (int) c));
         else b.append(c);
      }
      return b.toString();
   }

   static String res(java.util.function.Supplier<UUID> f) {
      try { UUID u = f.get(); return "ok " + u + " " + u.getMostSignificantBits() + " " + u.getLeastSignificantBits() + " " + u.hashCode(); }
      catch (RuntimeException e) { return "err " + esc(String.valueOf(e.getMessage())); }
   }

   public static void main(String[] args) {
      StringBuilder out = new StringBuilder();
      String[] names = {"", "Notch", "jeb_", "Dinnerbone", "a", "Player123", "A_very_long_name_exceeding_16", "\u00e9\u00e8", "\u4e2d\u6587", "\ud83d\ude00x", "with space", "Steve", "Alex", "\u0000", "x".repeat(1000)};
      for (String n : names) {
         UUID u = UUIDUtil.createOfflinePlayerUUID(n);
         int[] arr = UUIDUtil.uuidToIntArray(u);
         StringBuilder bytes = new StringBuilder();
         for (byte b : UUIDUtil.uuidToByteArray(u)) bytes.append(String.format(java.util.Locale.ROOT, "%02x", b));
         out.append("offline\t").append(esc(n)).append('\t').append(u).append('\t').append(u.getMostSignificantBits()).append('\t').append(u.getLeastSignificantBits())
            .append('\t').append(Arrays.toString(arr).replace(" ", "")).append('\t').append(bytes).append('\t').append(UndashedUuid.toString(u))
            .append('\t').append(u.hashCode()).append('\t').append(u.version()).append('\n');
      }
      Random r = new Random(42);
      for (int i = 0; i < 100; i++) {
         int[] a = {r.nextInt(), r.nextInt(), r.nextInt(), r.nextInt()};
         if (i % 10 == 0) a = new int[]{-1, 0, Integer.MIN_VALUE, i};
         UUID u = UUIDUtil.uuidFromIntArray(a);
         out.append("ints\t").append(Arrays.toString(a).replace(" ", "")).append('\t').append(u).append('\t').append(Arrays.toString(UUIDUtil.uuidToIntArray(u)).replace(" ", "")).append('\n');
      }
      String[] strs = {
         "", "a", "1-2-3-4-5", "+1-2-3-4-5", "-1-2-3-4-5", "1--3-4-5", "1-2-3-4-5-6", "zz-2-3-4-5", "123e4567-e89b-12d3-a456-426614174000", "123E4567-E89B-12D3-A456-426614174000",
         "123e4567-e89b-12d3-a456-4266141740001", "ffffffffffffffffff-2-3-4-5", "1-2-3-4-ffffffffffffffff", "7fffffffffffffff-2-3-4-5", "-8000000000000000-2-3-4-5", "1-2-3-4-0x5", "1-2-3-4-", "-1-2-3-4",
         "+-1-2-3-4", "1-2-3-4--", "1-2-3-+-5", "1-2-3--5-5", "ffffffff-ffff-ffff-ffff-ffffffffffff", "00000000-0000-0000-0000-000000000000", "ffffffffffffffff-ffff-ffff-ffff-ffffffffffff",
         "12345678-1234-1234-1234-1234567890ab", "12345678123412341234123456789 0ab", "12345678123412341234123456789012", "1-1-1-1-1", "1-1-1-1", "-----", "----", "\u00e9-2-3-4-5", "1-2-3-4-\ud83d\ude00",
         " 1-2-3-4-5", "1-2-3-4-5 ", "0x1-2-3-4-5", "1-2-3-4-0000000000001"
      };
      for (String s : strs) out.append("from\t").append(esc(s)).append('\t').append(res(() -> UUID.fromString(s))).append('\n');
      String[] und = {
         "", "123e4567e89b12d3a456426614174000", "123e4567-e89b-12d3-a456-426614174000", "123E4567E89B12D3A456426614174000", "123e4567e89b12d3a45642661417400", "123e4567e89b12d3a4564266141740000",
         "xx123e4567e89b12d3a456426614174000", "__________________________________", "123e4567e89b12d3a456426614174000zz", "123e4567e89b-12d3a456426614174000", "a b123e4567e89b12d3a456426614174000",
         "ffffffffffffffffffffffffffffffff", "00000000000000000000000000000000", "123e4567e89b12d3a456426614174-00"
      };
      for (String s : und) {
         out.append("undashed\t").append(esc(s)).append('\t').append(res(() -> UndashedUuid.fromString(s))).append('\t').append(res(() -> UndashedUuid.fromStringLenient(s))).append('\n');
      }
      System.out.print(out);
   }
}
