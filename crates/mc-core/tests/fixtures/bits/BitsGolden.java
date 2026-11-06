// Generates bits.txt.
// Usage: scripts/vanilla-java.sh -o crates/mc-core/tests/fixtures/bits/bits.txt crates/mc-core/tests/fixtures/bits/BitsGolden.java
// Output: crates/mc-core/tests/fixtures/bits/bits.txt
// `case <bits> <size> <acc> <unpack_hash> <raw_hash> <values_ctor_raw_hash> <raw|->` : ops driven by splitmix64 (same stream in the Rust test).
// `err <bits> <size> <len> = <message>` : data-array constructor length validation. `bad_bits <bits> = <message>`.
import java.util.*;
import net.minecraft.util.SimpleBitStorage;

public class BitsGolden {
   static long state;

   static long next() {
      long z = (state += 0x9E3779B97F4A7C15L);
      z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
      z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
      return z ^ (z >>> 31);
   }

   static long mix(long h, long x) { return h * 1000003L + x; }

   public static void main(String[] args) {
      StringBuilder out = new StringBuilder();
      int[] sizes = {0, 1, 7, 15, 16, 17, 63, 64, 65, 100, 1000, 4096};
      for (int bits = 1; bits <= 32; bits++) {
         for (int size : sizes) {
            state = bits * 1000L + size;
            SimpleBitStorage s = new SimpleBitStorage(bits, size);
            long mask = (1L << bits) - 1;
            long acc = 0;
            if (size > 0) {
               for (int step = 0; step < 300; step++) {
                  long op = next() % 8;
                  int idx = (int) Long.remainderUnsigned(next(), size);
                  int val = (int) ((next() >>> 17) & mask & 0x7FFFFFFFL);
                  if (op < 3) s.set(idx, val);
                  else if (op < 6) acc = mix(acc, s.getAndSet(idx, val));
                  else if (op == 6) acc = mix(acc, s.get(idx));
                  else if (next() % 16 == 0) s.fill(val);
                  else acc = mix(acc, s.get(idx));
               }
            }
            int[] un = new int[size];
            s.unpack(un);
            long uh = 0;
            for (int v : un) uh = mix(uh, v);
            long[] gall = {0};
            int[] cnt = {0};
            s.getAll(v -> { gall[0] = mix(gall[0], v); cnt[0]++; });
            if (gall[0] != uh || cnt[0] != size) throw new IllegalStateException("getAll != unpack");
            long[] raw = s.getRaw();
            long rh = 0;
            for (long v : raw) rh = mix(rh, v);
            // values ctor: same contents (raw may differ in unused padding bits left by fill)
            SimpleBitStorage fromValues = new SimpleBitStorage(bits, size, un);
            int[] un2 = new int[size];
            fromValues.unpack(un2);
            if (!Arrays.equals(un, un2)) throw new IllegalStateException("values ctor mismatch");
            long fh = 0;
            for (long v : fromValues.getRaw()) fh = mix(fh, v);
            // copy
            if (!Arrays.equals(s.copy().getRaw(), raw)) throw new IllegalStateException("copy mismatch");
            out.append("case ").append(bits).append(' ').append(size).append(' ').append(acc).append(' ').append(uh).append(' ').append(rh).append(' ').append(fh).append(' ');
            if (raw.length > 0 && raw.length <= 40) {
               StringJoiner j = new StringJoiner(",");
               for (long v : raw) j.add(Long.toString(v));
               out.append(j);
            } else out.append('-');
            out.append('\n');
            // data ctor with wrong length
            try {
               new SimpleBitStorage(bits, size, new long[raw.length + 1]);
               out.append("err ").append(bits).append(' ').append(size).append(' ').append(raw.length + 1).append(" = ok\n");
            } catch (RuntimeException ex) {
               out.append("err ").append(bits).append(' ').append(size).append(' ').append(raw.length + 1).append(" = ").append(ex.getMessage()).append('\n');
            }
         }
      }
      for (int bits : new int[]{0, 33, -1}) {
         try { new SimpleBitStorage(bits, 4); out.append("bad_bits ").append(bits).append(" = ok\n"); }
         catch (RuntimeException ex) { out.append("bad_bits ").append(bits).append(" = ").append(ex.getMessage()).append('\n'); }
      }
      // out-of-range access messages
      SimpleBitStorage s = new SimpleBitStorage(4, 10);
      try { s.set(10, 1); } catch (RuntimeException ex) { out.append("oob_index = ").append(ex.getMessage()).append('\n'); }
      try { s.set(1, 16); } catch (RuntimeException ex) { out.append("oob_value = ").append(ex.getMessage()).append('\n'); }
      System.out.print(out);
   }
}
