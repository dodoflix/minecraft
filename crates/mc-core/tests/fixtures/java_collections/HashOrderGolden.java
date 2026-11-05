import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.objects.Object2ObjectOpenHashMap;
import java.nio.file.*;
import java.util.*;

/**
 * Golden generator for mc_core::java_collections: the real iteration order of java.util.HashMap/HashSet (default
 * capacity, `new HashMap<>(n)`, copy constructors) and fastutil Object2ObjectOpenHashMap / Int2ObjectOpenHashMap after a
 * sequence of puts. Line: kind TAB capacity-or-expected TAB keys (US-separated) TAB iteration order (US-separated).
 * kinds: hashmap (capacity n), hashmap-copy (capacity = that of the source map the copy iterates), hashset-copy, fast (expected n), fastint (expected n).
 */
public class HashOrderGolden {
   static String join(Collection<?> c) {
      StringBuilder sb = new StringBuilder();
      for (Object o : c) sb.append(sb.length() > 0 ? "\u001f" : "").append(o);
      return sb.toString();
   }

   static String key(Random r, int alphabet, int maxLen) {
      int len = 1 + r.nextInt(maxLen);
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < len; i++) sb.append((char)('a' + r.nextInt(alphabet)));
      return sb.toString();
   }

   public static void main(String[] args) throws Exception {
      Random r = new Random(20260305);
      StringBuilder out = new StringBuilder();
      for (int round = 0; round < 600; round++) {
         int n = round < 100 ? 1 + round % 20 : 1 + r.nextInt(round < 400 ? 40 : 300);
         int alphabet = 2 + r.nextInt(24);
         LinkedHashSet<String> keys = new LinkedHashSet<>();
         for (int i = 0; i < n; i++) keys.add(round % 3 == 0 ? "minecraft:" + key(r, alphabet, 12) : key(r, alphabet, 8));
         List<String> ks = new ArrayList<>(keys);
         int cap = new int[] {1, 2, 4, 16, 16, 32, 64}[r.nextInt(7)];
         Map<String, Integer> m = new HashMap<>(cap);
         for (String k : ks) m.put(k, 0);
         out.append("hashmap\t").append(cap).append('\t').append(join(ks)).append('\t').append(join(m.keySet())).append('\n');
         Map<String, Integer> d = new HashMap<>();
         for (String k : ks) d.put(k, 0);
         out.append("hashmap\t16\t").append(join(ks)).append('\t').append(join(d.keySet())).append('\n');
         out.append("hashmap-copy\t").append(cap).append("\t").append(join(ks)).append('\t').append(join(new HashMap<>(m).keySet())).append('\n');
         out.append("hashset-copy\t0\t").append(join(ks)).append('\t').append(join(new HashSet<>(ks).stream().toList())).append('\n');
         int expected = new int[] {0, 1, 2, 3, 16, 16, 100}[r.nextInt(7)];
         Object2ObjectOpenHashMap<String, Integer> f = new Object2ObjectOpenHashMap<>(expected);
         List<String> puts = new ArrayList<>(ks);
         for (int i = 0; i < ks.size() / 3; i++) puts.add(ks.get(r.nextInt(ks.size())));
         for (String k : puts) f.put(k, 0);
         out.append("fast\t").append(expected).append('\t').append(join(puts)).append('\t').append(join(f.keySet())).append('\n');
         Int2ObjectOpenHashMap<String> fi = new Int2ObjectOpenHashMap<>(expected);
         List<Integer> ints = new ArrayList<>();
         for (int i = 0; i < n; i++) ints.add(round % 2 == 0 ? r.nextInt(60) - 10 : r.nextInt());
         for (int k : ints) fi.put(k, "");
         out.append("fastint\t").append(expected).append('\t').append(join(ints)).append('\t').append(join(fi.keySet())).append('\n');
      }
      Files.writeString(Path.of(args[0]), out.toString());
   }
}
