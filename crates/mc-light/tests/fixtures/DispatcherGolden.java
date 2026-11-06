import java.io.*;
import java.lang.reflect.*;
import java.util.*;
import java.util.concurrent.CompletableFuture;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.game.ClientboundLightUpdatePacketData;
import net.minecraft.server.level.*;
import net.minecraft.util.thread.ConsecutiveExecutor;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.lighting.*;
import net.minecraft.world.level.material.FluidState;

/**
 * Drives the real vanilla ThreadedLevelLightEngine behind its real ChunkTaskDispatcher / ConsecutiveExecutor and records
 * everything observable about the task order.
 * Usage: scripts/vanilla-java.sh -o crates/mc-light/tests/fixtures/dispatcher_golden.txt crates/mc-light/tests/fixtures/DispatcherGolden.java crates/mc-light/tests/fixtures/dispatcher_golden.txt
 * Output: crates/mc-light/tests/fixtures/dispatcher_golden.txt
 *
 * Vanilla runs the dispatcher and the light executor on pool threads (a race). This harness replaces the pool by two
 * manual FIFO executors and a fixed schedule: PUMP = run the dispatcher executor and the light executor until both are
 * idle (every submission is popped through the priority queue into lightTasks, the 1000-task inline runUpdate included),
 * then tryScheduleUpdate() and run the light executor (runUpdate: PRE tasks, runLightUpdates, POST tasks).
 *
 * Format: SCEN <name> <hasSky> <minY> <height> <gridSize>; SEC lines (initial blocks); INIT; then script lines replayed by
 * the Rust test: QL cx cz level|-  (queue level of a visible chunk; absent = 34), INITLIGHT/LIGHTCHUNK cx cz lighted,
 * SET x y z stateId (ProtoChunk.setBlockState through the threaded engine), BARRIER cx cz, PROP/ENABLE/RETAIN/SECT/STATUS,
 * PUMP. After each PUMP the expectations: DONE <completion tokens in order>, UPD <n> <hash of sorted onLightUpdate
 * sections>, CH <cx> <cz> <hash of all light layers> <light packet hash>.
 */
public class DispatcherGolden {
    static PrintWriter out;

    static PalettedContainerFactory factory() {
        var blocks = Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
        IdMapper<Holder<Biome>> ids = new IdMapper<>();
        Holder<Biome> dummy = Holder.direct(null);
        ids.add(dummy);
        var biomes = Strategy.createForBiomes(ids);
        return new PalettedContainerFactory(blocks, Blocks.AIR.defaultBlockState(), null, biomes, dummy, null);
    }

    static long fnv(byte[] b) {
        long h = 0xcbf29ce484222325L;
        for (byte x : b) { h ^= (x & 0xff); h *= 0x100000001b3L; }
        return h;
    }

    static class World implements LightChunkGetter, BlockGetter {
        final int minY, height;
        final Map<Long, ProtoChunk> chunks = new HashMap<>();
        final TreeSet<Long> updated = new TreeSet<>();
        World(int minY, int height) { this.minY = minY; this.height = height; }
        @Override public LightChunk getChunkForLighting(int x, int z) { return chunks.get(ChunkPos.pack(x, z)); }
        @Override public void onLightUpdate(LightLayer layer, SectionPos pos) { updated.add(pos.asLong() * 2 + (layer == LightLayer.SKY ? 1 : 0)); }
        @Override public BlockGetter getLevel() { return this; }
        @Override public int getHeight() { return height; }
        @Override public int getMinY() { return minY; }
        @Override public BlockEntity getBlockEntity(BlockPos pos) { return null; }
        @Override public BlockState getBlockState(BlockPos pos) {
            ProtoChunk c = chunks.get(ChunkPos.pack(pos.getX() >> 4, pos.getZ() >> 4));
            return c == null ? Blocks.BEDROCK.defaultBlockState() : c.getBlockState(pos);
        }
        @Override public FluidState getFluidState(BlockPos pos) { return getBlockState(pos).getFluidState(); }
    }

    static sun.misc.Unsafe unsafe;
    static it.unimi.dsi.fastutil.longs.Long2ObjectLinkedOpenHashMap<ChunkHolder> visible;
    static Field queueLevelField;

    /** `ChunkMap.getChunkQueueLevel` reads the visible holder's queue level: holders allocated without their constructor. */
    static void setQueueLevel(long pos, int level) throws Exception {
        ChunkHolder h = (ChunkHolder) unsafe.allocateInstance(ChunkHolder.class);
        queueLevelField.setInt(h, level);
        visible.put(pos, h);
    }

    static final ArrayDeque<Runnable> DISPATCH = new ArrayDeque<>(), LIGHT = new ArrayDeque<>();
    static World world;
    static ThreadedLevelLightEngine engine;
    static ChunkTaskDispatcher dispatcher;
    static List<String> done = new ArrayList<>();
    static Field lightTasksField;
    static Method updateChunkStatus;

    static boolean busy() throws Exception {
        List<?> tasks = (List<?>) lightTasksField.get(engine);
        return dispatcher.hasWork() || engine.hasLightWork() || !tasks.isEmpty() || !DISPATCH.isEmpty() || !LIGHT.isEmpty();
    }

    static void pump() throws Exception {
        world.updated.clear();
        done.clear();
        boolean progress;
        pumpNo++;
        do {
            progress = false;
            while (!DISPATCH.isEmpty()) { DISPATCH.poll().run(); progress = true; if (DEBUG) dumpQueue(); }
            while (!LIGHT.isEmpty()) { LIGHT.poll().run(); progress = true; }
        } while (progress);
        engine.tryScheduleUpdate();
        while (!LIGHT.isEmpty()) LIGHT.poll().run();
        while (!DISPATCH.isEmpty()) DISPATCH.poll().run();
        out.println("PUMP");
        out.println("DONE " + String.join(" ", done));
        byte[] b = new byte[world.updated.size() * 8];
        int i = 0;
        for (long l : world.updated) { for (int s = 56; s >= 0; s -= 8) b[i++] = (byte) (l >> s); }
        out.println("UPD " + world.updated.size() + " " + Long.toHexString(fnv(b)));
        List<Long> keys = new ArrayList<>(world.chunks.keySet());
        Collections.sort(keys);
        for (long k : keys) {
            int cx = ChunkPos.getX(k), cz = ChunkPos.getZ(k);
            java.io.ByteArrayOutputStream all = new java.io.ByteArrayOutputStream();
            for (LightLayer layer : new LightLayer[]{LightLayer.BLOCK, LightLayer.SKY}) {
                for (int sy = engine.getMinLightSection(); sy < engine.getMaxLightSection(); sy++) {
                    DataLayer d = engine.getLayerListener(layer).getDataLayerData(SectionPos.of(cx, sy, cz));
                    if (d == null) all.write(0); else { all.write(1); all.write(d.copy().getData()); }
                }
            }
            io.netty.buffer.ByteBuf buf = io.netty.buffer.Unpooled.buffer();
            ClientboundLightUpdatePacketData.STREAM_CODEC.encode(buf, new ClientboundLightUpdatePacketData(new ChunkPos(cx, cz), engine, null, null));
            byte[] p = new byte[buf.readableBytes()]; buf.readBytes(p);
            out.println("CH " + cx + " " + cz + " " + Long.toHexString(fnv(all.toByteArray())) + " " + Long.toHexString(fnv(p)));
        }
    }

    static int pumpNo = 0;
    static final boolean DEBUG = System.getenv("DDBG") != null;
    static void dumpQueue() throws Exception {
        Field qf = ChunkTaskDispatcher.class.getDeclaredField("queue"); qf.setAccessible(true);
        Object q = qf.get(dispatcher);
        Field lf = q.getClass().getDeclaredField("queuesPerPriority"); lf.setAccessible(true);
        List<?> qs = (List<?>) lf.get(q);
        StringBuilder sb = new StringBuilder("P" + pumpNo + " Q");
        for (int i = 0; i < qs.size(); i++) {
            var m = (it.unimi.dsi.fastutil.longs.Long2ObjectLinkedOpenHashMap<?>) qs.get(i);
            if (!m.isEmpty()) sb.append(" ").append(i).append("=").append(m.keySet());
        }
        System.err.println(sb);
    }

    static void drain() throws Exception {
        int guard = 0;
        while (busy()) { pump(); if (++guard > 50) throw new IllegalStateException("light never settles"); }
    }

    static Map<String, Block> BLOCKS = new HashMap<>();
    static BlockState st(String n) { return BLOCKS.get(n).defaultBlockState(); }

    static void set(ProtoChunk c, int x, int y, int z, BlockState s) {
        c.getSection(c.getSectionIndex(y)).setBlockState(x & 15, y & 15, z & 15, s);
    }

    static void generate(int n, Random r) {
        BlockState stone = st("stone"), dirt = st("dirt"), grass = st("grass_block"), air = Blocks.AIR.defaultBlockState();
        String[] lights = {"glowstone", "torch", "lantern", "sea_lantern", "shroomlight"};
        for (int cz = 0; cz < n; cz++) for (int cx = 0; cx < n; cx++) {
            ProtoChunk c = world.chunks.get(ChunkPos.pack(cx, cz));
            for (int lx = 0; lx < 16; lx++) for (int lz = 0; lz < 16; lz++) {
                int x = cx * 16 + lx, z = cz * 16 + lz;
                int surf = 40 + (int) (6 * Math.sin(x / 6.0) + 5 * Math.cos(z / 5.0)) + r.nextInt(2);
                for (int y = world.minY; y <= surf; y++) set(c, x, y, z, y < surf - 3 ? stone : y < surf ? dirt : grass);
                if (r.nextInt(60) == 0) set(c, x, surf + 1, z, st(lights[r.nextInt(lights.length)]));
                if (r.nextInt(40) == 0) for (int y = surf + 3; y < surf + 6; y++) set(c, x, y, z, stone);
            }
        }
    }

    static String sections(ProtoChunk c) {
        StringBuilder all = new StringBuilder();
        for (int i = 0; i < c.getSectionsCount(); i++) {
            LevelChunkSection s = c.getSection(i);
            if (s.hasOnlyAir()) continue;
            StringBuilder sb = new StringBuilder();
            int run = 0, last = -1;
            for (int y = 0; y < 16; y++) for (int z = 0; z < 16; z++) for (int x = 0; x < 16; x++) {
                int id = Block.BLOCK_STATE_REGISTRY.getId(s.getBlockState(x, y, z));
                if (id == last) run++;
                else { if (run > 0) sb.append(run).append(':').append(last).append(','); last = id; run = 1; }
            }
            sb.append(run).append(':').append(last);
            all.append("SEC ").append(c.getPos().x()).append(' ').append(c.getSectionYFromSectionIndex(i)).append(' ').append(c.getPos().z()).append(' ').append(sb).append('\n');
        }
        return all.toString();
    }

    static void track(String token, CompletableFuture<?> f) { f.thenRun(() -> done.add(token)); }

    static int barrierId;

    static void scenario(String name, boolean sky, int minY, int height, int n, long seed, int rounds, int opsPerRound, int sets) throws Exception {
        Random r = new Random(seed);
        world = new World(minY, height);
        PalettedContainerFactory f = factory();
        List<ProtoChunk> cs = new ArrayList<>();
        for (int cz = 0; cz < n; cz++) for (int cx = 0; cx < n; cx++) {
            ProtoChunk c = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, world, f, null);
            world.chunks.put(ChunkPos.pack(cx, cz), c);
            cs.add(c);
        }
        generate(n, r);
        out.println("SCEN " + name + " " + sky + " " + minY + " " + height + " " + n);
        for (ProtoChunk c : cs) out.print(sections(c));
        DISPATCH.clear(); LIGHT.clear(); done.clear(); barrierId = 0;
        ConsecutiveExecutor light = new ConsecutiveExecutor(LIGHT::add, "light");
        dispatcher = new ChunkTaskDispatcher(light, DISPATCH::add);
        Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); uf.setAccessible(true);
        unsafe = (sun.misc.Unsafe) uf.get(null);
        ChunkMap fake = (ChunkMap) unsafe.allocateInstance(ChunkMap.class);
        visible = new it.unimi.dsi.fastutil.longs.Long2ObjectLinkedOpenHashMap<>();
        Field vf = ChunkMap.class.getDeclaredField("visibleChunkMap"); vf.setAccessible(true); vf.set(fake, visible);
        queueLevelField = ChunkHolder.class.getDeclaredField("queueLevel"); queueLevelField.setAccessible(true);
        engine = new ThreadedLevelLightEngine(world, fake, sky, light, dispatcher);
        lightTasksField = ThreadedLevelLightEngine.class.getDeclaredField("lightTasks"); lightTasksField.setAccessible(true);
        updateChunkStatus = ThreadedLevelLightEngine.class.getDeclaredMethod("updateChunkStatus", ChunkPos.class); updateChunkStatus.setAccessible(true);
        for (ProtoChunk c : cs) { c.initializeLightSources(); c.setLightEngine(engine); c.setPersistedStatus(ChunkStatus.LIGHT); }
        out.println("INIT");
        BlockState[] palette = {Blocks.AIR.defaultBlockState(), st("stone"), st("glowstone"), st("torch"), st("water"), st("glass"), st("oak_leaves"), st("lantern"), st("dirt")};
        int[] levelChoices = {0, 1, 2, 3, 7, 22, 31, 33, 34, 40, 44, 45};
        int hi = n * 16;
        for (int round = 0; round < rounds; round++) {
            int ops = opsPerRound;
            for (int o = 0; o < ops; o++) {
                int cx = r.nextInt(n), cz = r.nextInt(n);
                ProtoChunk c = world.chunks.get(ChunkPos.pack(cx, cz));
                ChunkPos pos = c.getPos();
                int kind = r.nextInt(100);
                if (round == 0 && o < n * n) { // everyone gets initialized first, in shuffled order
                    cx = o % n; cz = o / n; c = world.chunks.get(ChunkPos.pack(cx, cz)); pos = c.getPos(); kind = 0;
                }
                if (kind < 14) {
                    boolean lighted = r.nextInt(5) == 0;
                    out.println("INITLIGHT " + cx + " " + cz + " " + lighted);
                    track("I:" + cx + ":" + cz, engine.initializeLight(c, lighted));
                } else if (kind < 26) {
                    boolean lighted = r.nextInt(5) == 0;
                    out.println("LIGHTCHUNK " + cx + " " + cz + " " + lighted);
                    track("L:" + cx + ":" + cz, engine.lightChunk(c, lighted));
                } else if (kind < 52) {
                    int cnt = 1 + r.nextInt(3);
                    for (int i = 0; i < cnt; i++) {
                        int x = cx * 16 + r.nextInt(16), z = cz * 16 + r.nextInt(16), y = minY + 20 + r.nextInt(50);
                        BlockState s = palette[r.nextInt(palette.length)];
                        c.setBlockState(new BlockPos(x, y, z), s, 0);
                        out.println("SET " + x + " " + y + " " + z + " " + Block.BLOCK_STATE_REGISTRY.getId(s));
                    }
                } else if (kind < 66) {
                    out.println("BARRIER " + cx + " " + cz);
                    track("B:" + (barrierId++) + ":" + cx + ":" + cz, engine.waitForPendingTasks(cx, cz));
                } else if (kind < 72) {
                    out.println("PROP " + cx + " " + cz);
                    engine.propagateLightSources(pos);
                } else if (kind < 78) {
                    boolean b = r.nextBoolean();
                    out.println("ENABLE " + cx + " " + cz + " " + b);
                    engine.setLightEnabled(pos, b);
                } else if (kind < 83) {
                    boolean b = r.nextBoolean();
                    out.println("RETAIN " + cx + " " + cz + " " + b);
                    engine.retainData(pos, b);
                } else if (kind < 88) {
                    int sy = minY / 16 + r.nextInt(height / 16);
                    boolean b = r.nextBoolean();
                    out.println("SECT " + cx + " " + sy + " " + cz + " " + b);
                    engine.updateSectionStatus(SectionPos.of(cx, sy, cz), b);
                } else if (kind < 90) {
                    out.println("STATUS " + cx + " " + cz);
                    updateChunkStatus.invoke(engine, pos);
                } else {
                    int lvl = levelChoices[r.nextInt(levelChoices.length)];
                    out.println("QL " + cx + " " + cz + " " + lvl);
                    setQueueLevel(pos.pack(), lvl);
                }
            }
            for (int i = 0; i < sets; i++) { // bulk block changes (checkBlock tasks) to overflow the 1000-task batch
                int x = r.nextInt(hi), z = r.nextInt(hi), y = minY + 20 + r.nextInt(50);
                BlockState s = palette[r.nextInt(palette.length)];
                world.chunks.get(ChunkPos.pack(x >> 4, z >> 4)).setBlockState(new BlockPos(x, y, z), s, 0);
                out.println("SET " + x + " " + y + " " + z + " " + Block.BLOCK_STATE_REGISTRY.getId(s));
                if (i % 700 == 699) { int cx = r.nextInt(n), cz = r.nextInt(n); out.println("BARRIER " + cx + " " + cz); track("B:" + (barrierId++) + ":" + cx + ":" + cz, engine.waitForPendingTasks(cx, cz)); }
            }
            pump();
        }
        drain();
        out.println("ENDSCEN");
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        for (Block b : BuiltInRegistries.BLOCK) BLOCKS.put(BuiltInRegistries.BLOCK.getKey(b).getPath(), b);
        out = new PrintWriter(new BufferedWriter(new FileWriter(args[0])));
        // name, hasSky, minY, height, grid, seed, rounds, ops/round, bulk sets/round
        scenario("small", true, 0, 128, 3, 1L, 6, 14, 0);
        scenario("levels", true, 0, 128, 4, 2L, 8, 30, 0);
        scenario("nosky", false, 0, 128, 4, 3L, 6, 24, 0);
        scenario("deep", true, -64, 192, 3, 4L, 6, 20, 0);
        scenario("batch", true, 0, 128, 4, 5L, 4, 18, 2600);
        scenario("batch_nosky", false, 0, 128, 3, 6L, 3, 10, 1500);
        out.println("END");
        out.close();
    }
}
