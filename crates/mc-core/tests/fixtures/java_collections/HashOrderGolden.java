import com.google.common.collect.Sets;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.objects.Object2ObjectOpenHashMap;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.BlockPos;

/**
 * Golden generator for mc_core::java_collections: the real iteration order of java.util.HashMap/HashSet (default
 * capacity, `new HashMap<>(n)`, copy constructors) and fastutil Object2ObjectOpenHashMap / Int2ObjectOpenHashMap after a
 * sequence of puts. Line: kind TAB capacity-or-expected TAB keys (US-separated) TAB iteration order (US-separated).
 * kinds: hashmap (capacity n), hashmap-copy (capacity = that of the source map the copy iterates), hashset-copy, fast (expected n), fastint (expected n),
 * and with BlockPos keys (hashCode (y + z*31)*31 + x) and removals, where the keys column is an operation list ("S+x,y,z" put into the source map
 * that is copied with putAll into the pending map before any P operation, "P+x,y,z"/"P-x,y,z"/"PC" put/remove/clear on a HashMap, "L+"/"L-"/"LC" on an
 * Object2ObjectOpenHashMap with expected n): posmap (the pending map's keys), posfast (the live map's keys), posunion (ChunkAccess.getBlockEntitiesPos:
 * new HashSet(pending.keySet()) then addAll(live.keySet())).
 * Usage: crates/mc-core/tests/fixtures/java_collections/gen.sh
 * Output: crates/mc-core/tests/fixtures/java_collections/golden.txt.gz
 */
public class HashOrderGolden {
   static String join(Collection<?> c) {
      StringBuilder sb = new StringBuilder();
      for (Object o : c) sb.append(sb.length() > 0 ? "\u001f" : "").append(o);
      return sb.toString();
   }

   // iterator order (a fastutil key set's `stream()` walks its table the other way round)
   static String joinPos(Collection<BlockPos> c) {
      List<String> l = new ArrayList<>();
      for (BlockPos p : c) l.add(pos(p));
      return join(l);
   }

   static String key(Random r, int alphabet, int maxLen) {
      int len = 1 + r.nextInt(maxLen);
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < len; i++) sb.append((char)('a' + r.nextInt(alphabet)));
      return sb.toString();
   }

   static BlockPos randomPos(Random r, int cx, int cz, boolean wide, boolean collide) {
      if (collide) {
         // x + z - y is constant mod 16, so every key lands in one bucket of a 16-table (long bins: treeifyBin resizes small tables)
         int x = cx * 16 + r.nextInt(16);
         int z = cz * 16 + r.nextInt(16);
         return new BlockPos(x, ((x + z + 5) & 15) + 16 * r.nextInt(20) - 64, z);
      }
      if (wide) return new BlockPos(r.nextInt(2001) - 1000, r.nextInt(384) - 64, r.nextInt(2001) - 1000);
      // clustered like the block entities of one chunk: a few columns, y repeating at 31 steps so some hashes collide
      return new BlockPos(cx * 16 + r.nextInt(16), -64 + r.nextInt(12) * (r.nextBoolean() ? 1 : 31) + r.nextInt(3), cz * 16 + r.nextInt(16));
   }

   static String pos(BlockPos p) {
      return p.getX() + "," + p.getY() + "," + p.getZ();
   }

   static void posRound(Random r, int round, StringBuilder out) {
      boolean wide = round % 5 == 4;
      boolean collide = round % 6 == 5;
      int cx = r.nextInt(20) - 10;
      int cz = r.nextInt(20) - 10;
      int expected = new int[] {0, 1, 2, 3, 16, 16, 100}[r.nextInt(7)];
      int count = 1 + r.nextInt(round % 4 == 0 ? 120 : 40);
      List<String> ops = new ArrayList<>();
      List<BlockPos> known = new ArrayList<>();
      HashMap<BlockPos, Integer> source = new HashMap<>();
      HashMap<BlockPos, Integer> pending = new HashMap<>();
      Object2ObjectOpenHashMap<BlockPos, Integer> live = new Object2ObjectOpenHashMap<>(expected);
      if (round % 3 == 0) {
         int n = r.nextInt(30);
         for (int i = 0; i < n; i++) {
            BlockPos p = randomPos(r, cx, cz, wide, collide);
            source.put(p, 0);
            ops.add("S+" + pos(p));
         }
         pending.putAll(source);
      }
      for (int i = 0; i < count; i++) {
         int roll = r.nextInt(100);
         String target = r.nextBoolean() ? "P" : "L";
         if (roll < 55 || known.isEmpty()) {
            BlockPos p = randomPos(r, cx, cz, wide, collide);
            known.add(p);
            (target.equals("P") ? pending : live).put(p, i);
            ops.add(target + "+" + pos(p));
         } else if (roll < 97) {
            BlockPos p = r.nextInt(8) == 0 ? randomPos(r, cx, cz, wide, collide) : known.get(r.nextInt(known.size()));
            (target.equals("P") ? pending : live).remove(p);
            ops.add(target + "-" + pos(p));
         } else {
            (target.equals("P") ? pending : live).clear();
            ops.add(target + "C");
         }
      }
      String joined = join(ops);
      out.append("posmap\t").append(expected).append('\t').append(joined).append('\t').append(joinPos(pending.keySet())).append('\n');
      out.append("posfast\t").append(expected).append('\t').append(joined).append('\t').append(joinPos(live.keySet())).append('\n');
      Set<BlockPos> union = Sets.newHashSet(pending.keySet());
      union.addAll(live.keySet());
      out.append("posunion\t").append(expected).append('\t').append(joined).append('\t').append(joinPos(union)).append('\n');
   }

   public static void main(String[] args) throws Exception {
      Random r = new Random(20260305);
      StringBuilder out = new StringBuilder();
      Random posRandom = new Random(20260612);
      for (int round = 0; round < 400; round++) posRound(posRandom, round, out);
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
