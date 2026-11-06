// Golden generator for `fastutil::LongOpenHashMap::new_int()` (= vanilla ChunkMap.entityMap Int2ObjectOpenHashMap):
// scripted put/remove sequences with entity-id-like keys, printing the `keySet()` iteration order after each script.
// Usage: scripts/vanilla-java.sh -o crates/mc-game-core/tests/fixtures/int2object_order.txt crates/mc-game/tests/fixtures/Int2ObjectOrderGen.java
// Output: crates/mc-game-core/tests/fixtures/int2object_order.txt
import it.unimi.dsi.fastutil.ints.Int2ObjectMap;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import java.util.Random;

public class Int2ObjectOrderGen {
   public static void main(String[] args) {
      Random r = new Random(1234);
      for (int script = 0; script < 40; script++) {
         Int2ObjectMap<Object> map = new Int2ObjectOpenHashMap<>();
         StringBuilder ops = new StringBuilder();
         int n = 5 + r.nextInt(120);
         int range = script % 2 == 0 ? 40 : 100000;
         for (int i = 0; i < n; i++) {
            int k = 1 + r.nextInt(range);
            if (r.nextInt(3) == 0) {
               map.remove(k);
               ops.append('-').append(k).append(' ');
            } else {
               map.put(k, "x");
               ops.append('+').append(k).append(' ');
            }
         }
         StringBuilder out = new StringBuilder();
         for (int k : map.keySet()) out.append(k).append(' ');
         System.out.println(ops.toString().trim() + " | " + out.toString().trim());
      }
   }
}
