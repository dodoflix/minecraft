import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import net.minecraft.util.Mth;
import net.minecraft.util.RandomSource;
import net.minecraft.util.Util;

/** MinecraftServer.buildPlayerStatus' sample selection (offset + shuffle) with real Mth/Util/RandomSource. */
public class StatusGolden {
   public static void main(String[] args) {
      StringBuilder out = new StringBuilder();
      long[] seeds = {0L, 1L, 42L, -4172144997902289642L};
      int[] counts = {0, 1, 2, 11, 12, 13, 20, 47};
      for (long seed : seeds) {
         RandomSource random = RandomSource.create(seed);
         for (int n : counts) {
            // players are named p0..pN-1; every 4th hides itself from the list
            int sampleSize = Math.min(n, 12);
            ObjectArrayList<String> sample = new ObjectArrayList<>(sampleSize);
            int offset = Mth.nextInt(random, 0, n - sampleSize);
            for (int i = 0; i < sampleSize; i++) {
               int p = offset + i;
               sample.add(p % 4 == 3 ? "Anonymous Player" : "p" + p);
            }
            Util.shuffle(sample, random);
            out.append(seed).append(' ').append(n).append(' ').append(String.join(",", sample)).append('\n');
         }
      }
      try {
         java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), out.toString());
      } catch (java.io.IOException e) {
         throw new RuntimeException(e);
      }
   }
}
