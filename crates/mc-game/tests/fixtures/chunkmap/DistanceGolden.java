// Golden generator for DistanceManager ticket levels (loading + simulation + natural spawn counter) and the
// LongOpenHashMap iteration order.
// Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/chunkmap/distance_golden.txt.gz crates/mc-game/tests/fixtures/chunkmap/DistanceGolden.java crates/mc-game/tests/fixtures/chunkmap/distance_golden.txt.gz
// Output: crates/mc-game/tests/fixtures/chunkmap/distance_golden.txt.gz
import it.unimi.dsi.fastutil.longs.*;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.*;
import net.minecraft.core.SectionPos;
import net.minecraft.server.level.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.TicketStorage;

public class DistanceGolden {
    static final Map<Long, ChunkHolder> holders = new HashMap<>();
    static final LongOpenHashSet toDrop = new LongOpenHashSet();
    static final List<String> log = new ArrayList<>();
    static final LevelHeightAccessor HEIGHT = new LevelHeightAccessor() {
        public int getHeight() { return 384; }
        public int getMinY() { return -64; }
    };

    static long seed = 12345;
    static int next(int bound) {
        seed = (seed * 6364136223846793005L + 1442695040888963407L);
        return (int) (((seed >>> 33) & 0x7fffffffL) % bound);
    }

    static Object field(Object o, String name) throws Exception {
        Field f = DistanceManager.class.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(o);
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        TicketStorage tickets = new TicketStorage();
        DistanceManager dm = new DistanceManager(tickets, Runnable::run, Runnable::run) {
            @Override protected boolean isChunkToRemove(long node) { return toDrop.contains(node); }
            @Override protected ChunkHolder getChunk(long node) { return holders.get(node); }
            @Override protected ChunkHolder updateChunkScheduling(long node, int level, ChunkHolder chunk, int oldLevel) {
                if (!ChunkLevel.isLoaded(oldLevel) && !ChunkLevel.isLoaded(level)) return chunk;
                if (chunk != null) chunk.setTicketLevel(level);
                if (chunk != null) { if (!ChunkLevel.isLoaded(level)) toDrop.add(node); else toDrop.remove(node); }
                if (ChunkLevel.isLoaded(level) && chunk == null) {
                    chunk = new ChunkHolder(ChunkPos.unpack(node), level, HEIGHT, null, null, null);
                    holders.put(node, chunk);
                }
                log.add("set " + ChunkPos.unpack(node).x() + "," + ChunkPos.unpack(node).z() + " " + level);
                return chunk;
            }
        };
        Object loading = field(dm, "loadingChunkTracker");
        Method runLoading = loading.getClass().getMethod("runDistanceUpdates", int.class);
        runLoading.setAccessible(true);
        Object sim = field(dm, "simulationChunkTracker");
        Method runSim = sim.getClass().getMethod("runAllUpdates");
        Object spawn = field(dm, "naturalSpawnChunkCounter");
        Method runSpawn = spawn.getClass().getMethod("runAllUpdates");
        runSpawn.setAccessible(true);
        Field chunksToUpdate = DistanceManager.class.getDeclaredField("chunksToUpdateFutures");
        chunksToUpdate.setAccessible(true);
        Method addPlayer = DistanceManager.class.getMethod("addPlayer", SectionPos.class, ServerPlayer.class);
        Method removePlayer = DistanceManager.class.getMethod("removePlayer", SectionPos.class, ServerPlayer.class);

        TicketType[] types = {TicketType.PLAYER_SPAWN, TicketType.DRAGON, TicketType.PLAYER_LOADING, TicketType.PLAYER_SIMULATION, TicketType.FORCED, TicketType.PORTAL, TicketType.ENDER_PEARL, TicketType.UNKNOWN};
        List<Object[]> active = new ArrayList<>();
        try (PrintWriter out = new PrintWriter(args[0])) {
            int players = 0;
            int[][] playerPos = new int[8][];
            for (int step = 0; step < 60; step++) {
                int ops = 1 + next(5);
                for (int i = 0; i < ops; i++) {
                    int kind = next(10);
                    if (kind < 5 || active.isEmpty()) {
                        TicketType t = types[next(types.length)];
                        int radius = next(6);
                        int x = next(30) - 15, z = next(30) - 15;
                        int level = ChunkLevel.byStatus(FullChunkStatus.FULL) - radius;
                        tickets.addTicket(new Ticket(t, level), new ChunkPos(x, z));
                        active.add(new Object[]{t, level, x, z});
                        out.println("A " + java.util.Arrays.asList(types).indexOf(t) + " " + level + " " + x + " " + z);
                    } else if (kind < 8) {
                        Object[] a = active.remove(next(active.size()));
                        tickets.removeTicket(new Ticket((TicketType) a[0], (Integer) a[1]), new ChunkPos((Integer) a[2], (Integer) a[3]));
                        out.println("R " + java.util.Arrays.asList(types).indexOf((TicketType) a[0]) + " " + a[1] + " " + a[2] + " " + a[3]);
                    } else if (kind == 8 && players < 8) {
                        int x, z;
                        boolean clash;
                        do {
                            // distinct chunks only: the harness uses null players, which collapse in the per-chunk set
                            x = next(200) - 100; z = next(200) - 100;
                            clash = false;
                            for (int q = 0; q < players; q++) if ((playerPos[q][0] >> 4) == (x >> 4) && (playerPos[q][1] >> 4) == (z >> 4)) clash = true;
                        } while (clash);
                        playerPos[players] = new int[]{x, z};
                        addPlayer.invoke(dm, SectionPos.of(x >> 4, 0, z >> 4), null);
                        out.println("P " + players + " " + (x >> 4) + " " + (z >> 4));
                        players++;
                    } else if (players > 0) {
                        players--;
                        int[] p = playerPos[players];
                        removePlayer.invoke(dm, SectionPos.of(p[0] >> 4, 0, p[1] >> 4), null);
                        out.println("Q " + players + " " + (p[0] >> 4) + " " + (p[1] >> 4));
                    }
                }
                log.clear();
                runSpawn.invoke(spawn);
                runSim.invoke(sim);
                runLoading.invoke(loading, Integer.MAX_VALUE);
                chunksToUpdate.setAccessible(true);
                ((Set<?>) chunksToUpdate.get(dm)).clear();
                out.println("U");
                for (String l : log) out.println("L " + l);
                final StringBuilder tsb = new StringBuilder("T");
                dm.forEachEntityTickingChunk(k -> tsb.append(' ').append(ChunkPos.unpack(k).x()).append(',').append(ChunkPos.unpack(k).z()));
                out.println(tsb);
                StringBuilder sb = new StringBuilder("S");
                LongIterator it = dm.getSpawnCandidateChunks();
                while (it.hasNext()) { long k = it.nextLong(); sb.append(' ').append(ChunkPos.unpack(k).x()).append(',').append(ChunkPos.unpack(k).z()); }
                out.println(sb);
                for (int z = -22; z <= 22; z += 1) {
                    sb = new StringBuilder("G");
                    for (int x = -22; x <= 22; x++) {
                        long k = ChunkPos.pack(x, z);
                        sb.append(' ').append(dm.getChunkLevel(k, false)).append('/').append(dm.getChunkLevel(k, true));
                    }
                    out.println(sb);
                }
                // process drops like ChunkMap.processUnloads
                for (LongIterator di = toDrop.iterator(); di.hasNext(); ) { holders.remove(di.nextLong()); di.remove(); }
                out.println("N " + dm.getNaturalSpawnChunkCount() + " " + tickets.getForceLoadedChunks().size());
            }
            // iteration order of LongOpenHashSet / Long2ByteOpenHashMap
            for (int round = 0; round < 40; round++) {
                it.unimi.dsi.fastutil.longs.Long2ByteOpenHashMap m = new it.unimi.dsi.fastutil.longs.Long2ByteOpenHashMap();
                LongOpenHashSet s = new LongOpenHashSet();
                int n = 1 + next(300);
                for (int i = 0; i < n * 2; i++) {
                    long k = ChunkPos.pack(next(40) - 20, next(40) - 20);
                    if (next(3) == 0) { m.remove(k); s.remove(k); out.println("fr " + k); }
                    else { m.put(k, (byte) next(30)); s.add(k); out.println("fp " + k); }
                }
                StringBuilder sb = new StringBuilder("FM");
                for (Long2ByteMap.Entry e : Long2ByteMaps.fastIterable(m)) sb.append(' ').append(e.getLongKey()).append(':').append(e.getByteValue());
                out.println(sb);
                sb = new StringBuilder("FS");
                for (LongIterator i2 = s.iterator(); i2.hasNext(); ) sb.append(' ').append(i2.nextLong());
                out.println(sb);
                // removal while iterating
                int r = 0;
                for (LongIterator i2 = s.iterator(); i2.hasNext(); ) { long k = i2.nextLong(); if (((k >> 3) & 1) == 0) { i2.remove(); r++; } }
                sb = new StringBuilder("FR");
                for (LongIterator i2 = s.iterator(); i2.hasNext(); ) sb.append(' ').append(i2.nextLong());
                out.println(sb);
                out.println("E");
            }
        }
    }
}
