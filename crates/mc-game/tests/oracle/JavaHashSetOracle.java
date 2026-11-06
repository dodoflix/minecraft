import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.levelgen.LegacyRandomSource;

/** Golden for tree::java_hash_set: iteration order of java.util.HashSet<BlockPos> for tree-shaped sets.
 *  Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/java_hash_set.txt crates/mc-game/tests/oracle/JavaHashSetOracle.java
 *  Output: crates/mc-game/tests/fixtures/java_hash_set.txt (stdout) */
public class JavaHashSetOracle {
    public static void main(String[] args) {
        LegacyRandomSource r = new LegacyRandomSource(12345L);
        int[] sizes = {1, 5, 12, 13, 14, 25, 50, 97, 100, 300, 800, 2000};
        for (int round = 0; round < 12; round++) {
            for (int n : sizes) {
                int ox = r.nextInt(8001) - 4000, oy = r.nextInt(300) - 64, oz = r.nextInt(8001) - 4000;
                int rx = 1 + r.nextInt(9), ry = 1 + r.nextInt(40), rz = 1 + r.nextInt(9);
                Set<BlockPos> set = new HashSet<>();
                for (int i = 0; i < n; i++) {
                    int x = ox + r.nextInt(2 * rx + 1) - rx, y = oy + r.nextInt(ry), z = oz + r.nextInt(2 * rz + 1) - rz;
                    set.add(new BlockPos(x, y, z));
                }
                long h = 0xcbf29ce484222325L;
                for (BlockPos p : set) {
                    h = (h ^ p.getX()) * 0x100000001b3L;
                    h = (h ^ p.getY()) * 0x100000001b3L;
                    h = (h ^ p.getZ()) * 0x100000001b3L;
                }
                System.out.println(n + " " + ox + " " + oy + " " + oz + " " + rx + " " + ry + " " + rz + " " + set.size() + " " + Long.toHexString(h));
            }
        }
    }
}
