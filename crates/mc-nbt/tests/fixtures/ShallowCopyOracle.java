import net.minecraft.nbt.*;
import java.io.*;
import java.nio.file.*;

/** `CompoundTag.shallowCopy()` pre-sizes the new HashMap from the entry count (JDK 19+: ceil(n / 0.75)); the key order of
 *  later inserts depends on that table size. Line: n TAB hex(named bytes of the copy after 7 more puts).
 *  Usage: scripts/vanilla-java.sh -o crates/mc-nbt/tests/fixtures/shallow_copy_golden.tsv crates/mc-nbt/tests/fixtures/ShallowCopyOracle.java crates/mc-nbt/tests/fixtures/shallow_copy_golden.tsv
 *  Output: crates/mc-nbt/tests/fixtures/shallow_copy_golden.tsv */
public class ShallowCopyOracle {
   public static void main(String[] args) throws Exception {
      StringBuilder out = new StringBuilder();
      for (int n = 1; n <= 60; n++) {
         CompoundTag c = new CompoundTag();
         for (int i = 0; i < n; i++) c.putInt("k" + i, i);
         java.lang.reflect.Method sc = CompoundTag.class.getDeclaredMethod("shallowCopy"); sc.setAccessible(true);
         CompoundTag d = (CompoundTag) sc.invoke(c);
         for (int i = 0; i < 7; i++) d.putInt("z" + i, i);
         ByteArrayOutputStream o = new ByteArrayOutputStream();
         NbtIo.write(d, new DataOutputStream(o));
         StringBuilder sb = new StringBuilder();
         for (byte x : o.toByteArray()) sb.append(String.format(java.util.Locale.ROOT, "%02x", x));
         out.append(n).append('\t').append(sb).append('\n');
      }
      Files.writeString(Path.of(args[0]), out.toString());
   }
}
