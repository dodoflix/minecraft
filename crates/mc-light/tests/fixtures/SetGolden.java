import it.unimi.dsi.fastutil.longs.*;
import java.io.*;
import java.util.*;

/** Iteration order of fastutil's LongOpenHashSet (used by the light engine) through adds, rehash, clear and trim. */
public class SetGolden {
    public static void main(String[] args) throws Exception {
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Random r = new Random(7);
        int[][] plans = {{512, 5}, {512, 40}, {512, 300}, {512, 700}, {512, 20}, {16, 100}, {16, 3}, {16, 1200}};
        for (int[] plan : plans) {
            boolean light = plan[0] == 512;
            LongOpenHashSet s = light ? new LongOpenHashSet(512, 0.5F) : new LongOpenHashSet();
            for (int round = 0; round < 3; round++) {
                out.println("NEW " + plan[0] + " " + round);
                for (int i = 0; i < plan[1]; i++) {
                    long v;
                    switch (r.nextInt(5)) {
                        case 0 -> v = ((long) (r.nextInt(64) - 32) & 0x3FFFFFFL) << 38 | ((long) (r.nextInt(64) - 32) & 0x3FFFFFFL) << 12 | ((r.nextInt(400)) - 64 & 0xFFFL);
                        case 1 -> v = r.nextInt(3) - 1;
                        case 2 -> v = r.nextLong();
                        default -> v = ((long) (r.nextInt(16) - 8) & 0x3FFFFFFL) << 38 | ((long) (r.nextInt(16)) & 0x3FFFFFFL) << 12 | (r.nextInt(48) & 0xFFFL);
                    }
                    out.println("ADD " + v + " " + s.add(v));
                }
                StringBuilder sb = new StringBuilder("ORDER " + s.size());
                LongIterator it = s.iterator();
                while (it.hasNext()) sb.append(' ').append(it.nextLong());
                out.println(sb);
                s.clear();
                if (light) s.trim(512);
            }
        }
        out.close();
    }
}
