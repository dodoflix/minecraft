import com.google.common.collect.Comparators;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import java.util.*;
import java.util.stream.*;
import net.minecraft.world.level.ChunkPos;

/** Golden for PlayerChunkSender.collectChunksToSend ordering (fastutil set iteration + Guava Comparators.least).
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/play_net/chunk_sender.golden crates/mc-game/tests/fixtures/play_net/ChunkSenderGolden.java crates/mc-game/tests/fixtures/play_net/chunk_sender.golden
 * Output: crates/mc-game/tests/fixtures/play_net/chunk_sender.golden */
public class ChunkSenderGolden {
    static String fmt(List<Long> l) {
        return l.stream().map(p -> ChunkPos.getX(p) + ":" + ChunkPos.getZ(p)).collect(Collectors.joining(","));
    }

    public static void main(String[] args) throws Exception {
        Random rnd = new Random(12345);
        StringBuilder out = new StringBuilder();
        for (int c = 0; c < 400; c++) {
            LongOpenHashSet set = new LongOpenHashSet();
            int radius = 1 + rnd.nextInt(14);
            int n = 1 + rnd.nextInt(2 * radius * radius + 8);
            List<String> ops = new ArrayList<>();
            List<Long> added = new ArrayList<>();
            for (int i = 0; i < n; i++) {
                int x = rnd.nextInt(2 * radius + 1) - radius, z = rnd.nextInt(2 * radius + 1) - radius;
                long p = ChunkPos.pack(x, z);
                set.add(p);
                added.add(p);
                ops.add("a:" + x + ":" + z);
                if (rnd.nextInt(5) == 0 && !added.isEmpty()) {
                    long r = added.get(rnd.nextInt(added.size()));
                    set.remove(r);
                    ops.add("r:" + ChunkPos.getX(r) + ":" + ChunkPos.getZ(r));
                }
            }
            int cx = rnd.nextInt(5) - 2, cz = rnd.nextInt(5) - 2;
            ChunkPos player = new ChunkPos(cx, cz);
            int k = 1 + rnd.nextInt(64);
            List<Long> least = set.stream().collect(Comparators.least(k, Comparator.comparingInt(player::distanceSquared)));
            List<Long> sorted = set.longStream().boxed().sorted(Comparator.comparingInt(player::distanceSquared)).collect(Collectors.toList());
            List<Long> iter = set.longStream().boxed().collect(Collectors.toList());
            out.append(String.join(",", ops)).append(';').append(cx).append(',').append(cz).append(';').append(k).append(';')
                .append(set.size()).append(';').append(fmt(iter)).append(';').append(fmt(least)).append(';').append(fmt(sorted)).append('\n');
        }
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), out.toString());
    }
}
