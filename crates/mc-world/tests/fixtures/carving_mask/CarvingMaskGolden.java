import java.io.*;
import java.util.*;
import net.minecraft.world.level.chunk.CarvingMask;

/**
 * Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/carving_mask/golden.txt crates/mc-world/tests/fixtures/carving_mask/CarvingMaskGolden.java crates/mc-world/tests/fixtures/carving_mask/golden.txt
 * Output: crates/mc-world/tests/fixtures/carving_mask/golden.txt
 */
public class CarvingMaskGolden {
    public static void main(String[] args) throws Exception {
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Random rnd = new Random(7);
        int[][] cfgs = {{-63, 319}, {1, 255}, {0, 15}, {-64, 0}, {5, 5}, {-20, 150}, {0, 63}};
        for (int[] cfg : cfgs) {
            int minY = cfg[0], maxY = cfg[1];
            for (int variant = 0; variant < 4; variant++) {
                CarvingMask mask = new CarvingMask(minY, maxY);
                out.println("CFG " + minY + " " + maxY + " " + mask.minY() + " " + mask.maxY() + " empty=" + mask.isEmpty());
                int nOps = switch (variant) { case 0 -> 3; case 1 -> 60; case 2 -> 400; default -> 700; };
                int height = maxY - minY + 1;
                for (int i = 0; i < nOps; i++) {
                    int kind = rnd.nextInt(6);
                    int x = rnd.nextInt(16), z = rnd.nextInt(16);
                    if (kind < 3 || variant == 0) {
                        int y = minY + rnd.nextInt(height);
                        mask.carve(x, y, z);
                        out.println("C " + x + " " + y + " " + z);
                    } else if (kind < 5) {
                        // vertical run in a column, sometimes reaching the ends so runs join neighbouring columns
                        int a = rnd.nextInt(4) == 0 ? minY : minY + rnd.nextInt(height);
                        int b = rnd.nextInt(4) == 0 ? maxY : minY + rnd.nextInt(height);
                        for (int y = Math.min(a, b); y <= Math.max(a, b); y++) mask.carve(x, y, z);
                        out.println("R " + x + " " + z + " " + Math.min(a, b) + " " + Math.max(a, b));
                    } else {
                        // whole column, optionally next column too
                        for (int dz = 0; dz < 1 + rnd.nextInt(3) && z + dz < 16; dz++)
                            { for (int y = minY; y <= maxY; y++) mask.carve(x, y, z + dz); out.println("R " + x + " " + (z + dz) + " " + minY + " " + maxY); }
                    }
                    if (variant == 0 && i == 0) out.println("EMPTY " + mask.isEmpty());
                }
                out.println("EMPTY " + mask.isEmpty());
                mask.visit((vx, vz, bottom, top) -> out.println("V " + vx + " " + vz + " " + bottom + " " + top));
                out.println("END");
            }
        }
        out.close();
    }
}
