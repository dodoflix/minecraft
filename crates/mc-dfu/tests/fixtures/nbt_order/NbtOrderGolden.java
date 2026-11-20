import com.mojang.datafixers.util.Pair;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;

/**
 * Golden generator for mc_dfu's NBT key order: the iteration order of a `CompoundTag` (a `java.util.HashMap`) that
 * `NbtOps` builds with `createMap` and then edits with `set` (`mergeToMap`: `shallowCopy()` then `put`) and `remove`
 * (`shallowCopy()` then `remove`). The keys collide on purpose: strings of equal hash code ("Aa" and "BB" blocks) and
 * strings that share the low bits of their spread hash, in numbers that make `HashMap` resize small tables and treeify
 * bins (9 or more keys in a bin of a table of 64 or more).
 * Line: initial keys (US-separated) TAB operations (US-separated: "+key" set, "-key" remove) TAB resulting key order (US-separated).
 * Usage: crates/mc-dfu/tests/fixtures/nbt_order/gen.sh
 * Output: crates/mc-dfu/tests/fixtures/nbt_order/golden.txt.gz
 */
public class NbtOrderGolden {
   static String join(Collection<String> c) {
      return String.join("\u001f", c);
   }

   static int spread(String s) {
      int h = s.hashCode();
      return h ^ (h >>> 16);
   }

   /** n distinct strings of equal hash code. */
   static List<String> fullCollision(Random r, int n, String prefix) {
      int blocks = 1;
      while ((1 << blocks) < n) blocks++;
      Set<String> out = new LinkedHashSet<>();
      while (out.size() < n) {
         StringBuilder sb = new StringBuilder(prefix);
         for (int i = 0; i < blocks; i++) sb.append(r.nextBoolean() ? "Aa" : "BB");
         out.add(sb.toString());
      }
      return new ArrayList<>(out);
   }

   /** n distinct strings whose spread hash agrees with `target` on the bits of `mask` (different hash codes). */
   static List<String> sameBucket(Random r, int n, int mask, int target) {
      Set<String> out = new LinkedHashSet<>();
      while (out.size() < n) {
         StringBuilder sb = new StringBuilder();
         int len = 3 + r.nextInt(5);
         for (int i = 0; i < len; i++) sb.append((char) ('a' + r.nextInt(26)));
         String s = sb.toString();
         if ((spread(s) & mask) == (target & mask)) out.add(s);
      }
      return new ArrayList<>(out);
   }

   static List<String> filler(Random r, int n) {
      Set<String> out = new LinkedHashSet<>();
      while (out.size() < n) {
         StringBuilder sb = new StringBuilder("f");
         int len = 2 + r.nextInt(6);
         for (int i = 0; i < len; i++) sb.append((char) ('a' + r.nextInt(26)));
         out.add(sb.toString());
      }
      return new ArrayList<>(out);
   }

   static Tag create(List<String> keys) {
      List<Pair<Tag, Tag>> pairs = new ArrayList<>();
      for (int i = 0; i < keys.size(); i++) pairs.add(Pair.of(StringTag.valueOf(keys.get(i)), IntTag.valueOf(i)));
      return NbtOps.INSTANCE.createMap(pairs.stream());
   }

   static String order(Tag t) {
      return join(new ArrayList<>(((CompoundTag) t).keySet()));
   }

   public static void main(String[] args) throws Exception {
      StringBuilder out = new StringBuilder();
      Random r = new Random(20261121);
      for (int round = 0; round < 900; round++) {
         List<String> pool = new ArrayList<>();
         int groups = 1 + r.nextInt(3);
         for (int g = 0; g < groups; g++) {
            int n = 4 + r.nextInt(round % 5 == 0 ? 40 : 14);
            switch (r.nextInt(3)) {
               case 0 -> pool.addAll(fullCollision(r, n, "k" + g + "_"));
               case 1 -> pool.addAll(sameBucket(r, n, new int[] {15, 31, 63, 127}[r.nextInt(4)], r.nextInt(128)));
               default -> {
                  pool.addAll(fullCollision(r, n / 2 + 1, "m" + g + "_"));
                  pool.addAll(sameBucket(r, n / 2 + 1, 63, r.nextInt(64)));
               }
            }
         }
         pool.addAll(filler(r, r.nextInt(round % 3 == 0 ? 70 : 12)));
         Collections.shuffle(pool, r);
         pool = new ArrayList<>(new LinkedHashSet<>(pool));
         int initialCount = r.nextInt(pool.size() + 1);
         List<String> initial = new ArrayList<>(pool.subList(0, initialCount));
         Tag tag = create(initial);
         List<String> ops = new ArrayList<>();
         int nOps = r.nextInt(round % 4 == 0 ? 60 : 25);
         int counter = 1000;
         for (int i = 0; i < nOps; i++) {
            String k = pool.get(r.nextInt(pool.size()));
            if (r.nextInt(100) < 60) {
               tag = NbtOps.INSTANCE.set(tag, k, IntTag.valueOf(counter++));
               ops.add("+" + k);
            } else {
               tag = NbtOps.INSTANCE.remove(tag, k);
               ops.add("-" + k);
            }
         }
         out.append(join(initial)).append('\t').append(join(ops)).append('\t').append(order(tag)).append('\n');
      }
      Files.writeString(Path.of(args[0]), out.toString());
   }
}
