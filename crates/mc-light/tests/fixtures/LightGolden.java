import java.io.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.lighting.*;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.network.protocol.game.ClientboundLightUpdatePacketData;

/**
 * Drives the real vanilla LevelLightEngine (sky + block) over real ProtoChunks and records everything observable.
 * Usage: scripts/vanilla-java.sh LightGolden.java out.txt
 *
 * Output (one scenario after the other):
 *   SCEN <name> <minY> <height> <hasSky> <gridMin> <gridSize>
 *   SEC <cx> <sy> <cz> <count:stateId,...>        initial blocks, y-major then z then x (only non-air sections)
 *   Then a script of steps; every step is replayed by the Rust test, `SNAP` lines are the expectations:
 *     INIT                       initializeLight(lighted=false) for all chunks, runUpdate, lightChunk(false), runUpdate
 *     OP x y z stateId           chunk.setBlockState (ProtoChunk path with light hooks)
 *     RUN                        runLightUpdates (expectation: RUNCOUNT n)
 *     UNLOAD cx cz / RELOAD cx cz   chunk leaves / re-enters the light engine with its saved light (lighted=true)
 *     SNAP <label>               followed by L/Q/SRC/PKT lines
 *   L <B|S> <cx> <sy> <cz> <fnv64 hex | ->   data layer of every light section of every loaded chunk
 *   Q <B|S> x y z v / Q R x y z skyDampen v   getLightValue / getRawBrightness(pos, skyDampen)
 *   SRC <cx> <cz> <fnv64>      ChunkSkyLightSources lowest source y of the 256 columns
 *   PKT <cx> <cz> <fnv64> <fnv64 filtered>   ClientboundLightUpdatePacketData bytes
 *   UPD <n> <fnv64>            sections reported through onLightUpdate during the last run (sorted)
 */
public class LightGolden {
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

    static Map<String, Block> BLOCKS = new HashMap<>();
    static List<BlockState> ALL_STATES = new ArrayList<>();
    static BlockState st(String name) {
        Block b = BLOCKS.get(name);
        if (b == null) throw new IllegalArgumentException(name);
        return b.defaultBlockState();
    }
    static BlockState anyState(String name, Random r) {
        List<BlockState> s = BLOCKS.get(name).getStateDefinition().getPossibleStates();
        return s.get(r.nextInt(s.size()));
    }

    static String[] LIGHTS_ALL = {"glowstone", "sea_lantern", "torch", "lantern", "soul_lantern", "lava", "fire", "magma_block", "shroomlight", "jack_o_lantern", "campfire",
        "candle", "light", "beacon", "redstone_lamp", "froglight", "ochre_froglight", "end_rod", "soul_torch", "wall_torch", "copper_bulb", "crying_obsidian", "respawn_anchor", "furnace"};
    static String[] SHAPES_ALL = {"oak_slab", "oak_stairs", "stone_brick_stairs", "snow", "farmland", "dirt_path", "oak_trapdoor", "oak_fence", "cobblestone_wall", "stone_slab",
        "soul_sand", "honey_block", "slime_block", "oak_door", "piston", "chest", "hopper", "cauldron", "composter", "lectern", "oak_leaves", "water", "kelp", "cobweb", "glass", "ice",
        "tinted_glass", "powder_snow", "barrier", "bedrock", "end_portal_frame", "dragon_egg", "amethyst_cluster", "pointed_dripstone", "scaffold", "azalea", "flower_pot", "sculk_sensor", "vine", "ladder"};

    static String[] LIGHTS, SHAPES;
    static String[] existing(String[] names) {
        List<String> l = new ArrayList<>();
        for (String n : names) { if (BLOCKS.containsKey(n)) l.add(n); else System.err.println("missing block " + n); }
        return l.toArray(new String[0]);
    }

    static void set(World w, int x, int y, int z, BlockState s) {
        ProtoChunk c = w.chunks.get(ChunkPos.pack(x >> 4, z >> 4));
        c.getSection(c.getSectionIndex(y)).setBlockState(x & 15, y & 15, z & 15, s);
    }

    static void generate(World w, String style, int g0, int n, Random r) {
        int lo = g0 * 16, hi = (g0 + n) * 16 - 1;
        int minY = w.minY, top = w.minY + w.height - 1;
        int base = style.equals("tall") ? 120 : style.equals("nether") ? 70 : style.equals("end") ? 40 : 62;
        BlockState stone = st("stone"), deep = st("deepslate"), dirt = st("dirt"), grass = st("grass_block"), water = st("water"), air = Blocks.AIR.defaultBlockState();
        BlockState netherrack = st("netherrack"), endstone = st("end_stone"), lava = st("lava");
        double a1 = 5 + r.nextInt(6), a2 = 3 + r.nextInt(5), p1 = 7 + r.nextInt(5), p2 = 5 + r.nextInt(7), p3 = r.nextDouble() * 6;
        for (int x = lo; x <= hi; x++) {
            for (int z = lo; z <= hi; z++) {
                int surf = base + (int) (a1 * Math.sin(x / p1 + p3) + a2 * Math.cos(z / p2) + r.nextInt(3));
                if (style.equals("flat")) surf = base + (((x >> 2) + (z >> 3)) % 2);
                for (int y = minY; y <= Math.min(surf, top); y++) {
                    BlockState s;
                    if (style.equals("nether")) {
                        double d = Math.sin(x / 5.0) + Math.sin(y / 4.0 + z / 6.0) + Math.cos((x + z) / 7.0) + Math.sin(y / 9.0);
                        s = (y <= 2 || y >= top - 3) ? st("bedrock") : d > 0.4 && y > 32 ? air : (d > 0.9 && y <= 32 ? lava : netherrack);
                    } else if (style.equals("end")) {
                        double d = Math.sin(x / 7.0) * Math.cos(z / 6.0) * 12;
                        if (y > base + d || y < base - Math.abs(d) * 1.5 - 1) continue;
                        s = endstone;
                    } else {
                        s = y <= minY + 2 ? (y == minY || r.nextBoolean() ? st("bedrock") : deep) : y < 0 ? deep : y < surf - 3 ? stone : y < surf ? dirt : grass;
                    }
                    set(w, x, y, z, s);
                }
                if (!style.equals("nether") && !style.equals("end")) {
                    for (int y = surf + 1; y <= 63 && y <= top; y++) set(w, x, y, z, water);
                    if (surf < 63 && r.nextInt(6) == 0 && 63 <= top) set(w, x, 63, z, st("ice"));
                }
            }
        }
        // caves
        int caves = style.equals("flat") ? 0 : 6;
        for (int i = 0; i < caves; i++) {
            int cx = lo + r.nextInt(hi - lo), cz = lo + r.nextInt(hi - lo), cy = Math.max(minY + 8, base - 8 - r.nextInt(30)), rad = 2 + r.nextInt(4);
            for (int dx = -rad; dx <= rad; dx++) for (int dy = -rad; dy <= rad; dy++) for (int dz = -rad; dz <= rad; dz++) {
                int x = cx + dx, y = cy + dy, z = cz + dz;
                if (dx * dx + dy * dy + dz * dz <= rad * rad && x >= lo && x <= hi && z >= lo && z <= hi && y > minY + 2 && y <= top) set(w, x, y, z, y < minY + 14 + 0 ? lava : st("cave_air"));
            }
        }
        // pillars to the sky, trees, buildings, light sources, shape blocks
        int features = style.equals("chaos") ? 400 : 140;
        for (int i = 0; i < features; i++) {
            int x = lo + r.nextInt(hi - lo + 1), z = lo + r.nextInt(hi - lo + 1);
            int y = Math.min(top - 1, Math.max(minY + 3, base + r.nextInt(24) - 6));
            switch (r.nextInt(8)) {
                case 0 -> { for (int dy = 0; dy < 4 + r.nextInt(5) && y + dy <= top; dy++) if (x >= lo && x <= hi) set(w, x, y + dy, z, st("oak_log")); }
                case 1 -> { for (int dx = -2; dx <= 2; dx++) for (int dz = -2; dz <= 2; dz++) for (int dy = 0; dy < 2; dy++) { int xx = x + dx, zz = z + dz; if (xx >= lo && xx <= hi && zz >= lo && zz <= hi && y + dy <= top && r.nextInt(5) != 0) set(w, xx, y + dy, zz, anyState("oak_leaves", r)); } }
                case 2 -> { for (int dx = 0; dx < 5; dx++) for (int dz = 0; dz < 5; dz++) for (int dy = 0; dy < 4; dy++) { int xx = x + dx, zz = z + dz; if (xx > hi || zz > hi || y + dy > top) continue; boolean wall = dx == 0 || dz == 0 || dx == 4 || dz == 4 || dy == 3; if (wall) set(w, xx, y + dy, zz, dy == 1 && (dx + dz) % 2 == 0 ? st("glass") : st("stone_bricks")); } }
                case 3, 4 -> { set(w, x, y, z, anyState(LIGHTS[r.nextInt(LIGHTS.length)], r)); }
                case 5 -> { for (int k = 0; k < 6; k++) { int xx = x + r.nextInt(5) - 2, zz = z + r.nextInt(5) - 2, yy = y + r.nextInt(3); if (xx >= lo && xx <= hi && zz >= lo && zz <= hi && yy <= top) set(w, xx, yy, zz, anyState(SHAPES[r.nextInt(SHAPES.length)], r)); } }
                case 6 -> { if (style.equals("chaos")) for (int k = 0; k < 30; k++) { int xx = x + r.nextInt(7) - 3, zz = z + r.nextInt(7) - 3, yy = y + r.nextInt(7) - 3; if (xx >= lo && xx <= hi && zz >= lo && zz <= hi && yy <= top && yy >= minY) set(w, xx, yy, zz, ALL_STATES.get(r.nextInt(ALL_STATES.size()))); } }
                default -> { int yy = y + r.nextInt(40); if (yy <= top) set(w, x, yy, z, anyState(SHAPES[r.nextInt(SHAPES.length)], r)); }
            }
        }
    }

    static void sections(World w, int g0, int n) {
        for (int cz = g0; cz < g0 + n; cz++) for (int cx = g0; cx < g0 + n; cx++) {
            ProtoChunk c = w.chunks.get(ChunkPos.pack(cx, cz));
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
                out.println("SEC " + cx + " " + (c.getSectionYFromSectionIndex(i)) + " " + cz + " " + sb);
            }
        }
    }

    static void snapshot(World w, LevelLightEngine engine, String label, Random q) {
        out.println("SNAP " + label);
        List<Long> keys = new ArrayList<>(w.chunks.keySet());
        Collections.sort(keys);
        for (long k : keys) {
            int cx = ChunkPos.getX(k), cz = ChunkPos.getZ(k);
            for (LightLayer layer : new LightLayer[]{LightLayer.BLOCK, LightLayer.SKY}) {
                for (int sy = engine.getMinLightSection(); sy < engine.getMaxLightSection(); sy++) {
                    DataLayer d = engine.getLayerListener(layer).getDataLayerData(SectionPos.of(cx, sy, cz));
                    out.println("L " + (layer == LightLayer.BLOCK ? "B" : "S") + " " + cx + " " + sy + " " + cz + " " + (d == null ? "-" : Long.toHexString(fnv(d.copy().getData()))));
                }
            }
            ChunkSkyLightSources src = w.chunks.get(k).getSkyLightSources();
            byte[] b = new byte[256 * 4];
            for (int i = 0; i < 256; i++) { int v = src.getLowestSourceY(i & 15, i >> 4); b[i * 4] = (byte) (v >> 24); b[i * 4 + 1] = (byte) (v >> 16); b[i * 4 + 2] = (byte) (v >> 8); b[i * 4 + 3] = (byte) v; }
            out.println("SRC " + cx + " " + cz + " " + Long.toHexString(fnv(b)));
            io.netty.buffer.ByteBuf buf = io.netty.buffer.Unpooled.buffer();
            ClientboundLightUpdatePacketData.STREAM_CODEC.encode(buf, new ClientboundLightUpdatePacketData(new ChunkPos(cx, cz), engine, null, null));
            byte[] p = new byte[buf.readableBytes()]; buf.readBytes(p);
            BitSet f = new BitSet(); for (int i = 0; i < engine.getLightSectionCount(); i += 3) f.set(i);
            BitSet f2 = new BitSet(); for (int i = 1; i < engine.getLightSectionCount(); i += 2) f2.set(i);
            io.netty.buffer.ByteBuf buf2 = io.netty.buffer.Unpooled.buffer();
            ClientboundLightUpdatePacketData.STREAM_CODEC.encode(buf2, new ClientboundLightUpdatePacketData(new ChunkPos(cx, cz), engine, f, f2));
            byte[] p2 = new byte[buf2.readableBytes()]; buf2.readBytes(p2);
            out.println("PKT " + cx + " " + cz + " " + Long.toHexString(fnv(p)) + " " + Long.toHexString(fnv(p2)));
        }
        int minC = Integer.MAX_VALUE, maxC = Integer.MIN_VALUE;
        for (long k : keys) { minC = Math.min(minC, Math.min(ChunkPos.getX(k), ChunkPos.getZ(k))); maxC = Math.max(maxC, Math.max(ChunkPos.getX(k), ChunkPos.getZ(k))); }
        int lo = minC * 16 - 4, hi = (maxC + 1) * 16 + 4;
        for (int i = 0; i < 120; i++) {
            int x = lo + q.nextInt(hi - lo), z = lo + q.nextInt(hi - lo), y = w.minY - 20 + q.nextInt(w.height + 40);
            BlockPos pos = new BlockPos(x, y, z);
            out.println("Q B " + x + " " + y + " " + z + " " + engine.getLayerListener(LightLayer.BLOCK).getLightValue(pos));
            out.println("Q S " + x + " " + y + " " + z + " " + engine.getLayerListener(LightLayer.SKY).getLightValue(pos));
            int damp = q.nextInt(4);
            out.println("Q R " + x + " " + y + " " + z + " " + damp + " " + engine.getRawBrightness(pos, damp));
        }
    }

    static void run(World w, LevelLightEngine engine) {
        w.updated.clear();
        int n = engine.runLightUpdates();
        if (engine.hasLightWork()) throw new IllegalStateException("light work left");
        out.println("RUNCOUNT " + n);
        byte[] b = new byte[w.updated.size() * 8];
        int i = 0;
        for (long l : w.updated) { for (int s = 56; s >= 0; s -= 8) b[i++] = (byte) (l >> s); }
        out.println("UPD " + w.updated.size() + " " + Long.toHexString(fnv(b)));
    }

    static void initLight(World w, LevelLightEngine engine, List<ProtoChunk> cs, boolean lighted) {
        for (ProtoChunk c : cs) {
            for (int i = 0; i < c.getSectionsCount(); i++) if (!c.getSection(i).hasOnlyAir()) engine.updateSectionStatus(SectionPos.of(c.getPos(), c.getSectionYFromSectionIndex(i)), false);
        }
        run(w, engine);
        for (ProtoChunk c : cs) { engine.setLightEnabled(c.getPos(), lighted); engine.retainData(c.getPos(), false); }
        if (!lighted) {
            for (ProtoChunk c : cs) engine.propagateLightSources(c.getPos());
        }
        run(w, engine);
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        for (Block b : BuiltInRegistries.BLOCK) BLOCKS.put(BuiltInRegistries.BLOCK.getKey(b).getPath(), b);
        for (BlockState s : Block.BLOCK_STATE_REGISTRY) ALL_STATES.add(s);
        LIGHTS = existing(LIGHTS_ALL);
        SHAPES = existing(SHAPES_ALL);
        out = new PrintWriter(new BufferedWriter(new FileWriter(args[0])));
        PalettedContainerFactory f = factory();
        // name, minY, height, hasSky, style, seed
        Object[][] scens = {
            {"overworld", -64, 384, true, "terrain", 11L},
            {"overworld2", -64, 384, true, "terrain", 12L},
            {"odd_height", -48, 160, true, "terrain", 13L},
            {"nether", 0, 256, false, "nether", 14L},
            {"nether_sky", 0, 256, true, "nether", 15L},
            {"end", 0, 256, true, "end", 16L},
            {"flat", -64, 384, true, "flat", 17L},
            {"chaos", -64, 384, true, "chaos", 18L},
            {"chaos_nosky", 0, 256, false, "chaos", 19L},
            {"tall", -128, 512, true, "tall", 20L},
        };
        for (Object[] sc : scens) {
            String name = (String) sc[0];
            int minY = (Integer) sc[1], height = (Integer) sc[2];
            boolean sky = (Boolean) sc[3];
            String style = (String) sc[4];
            Random r = new Random((Long) sc[5]);
            int g0 = -1, n = 3;
            out.println("SCEN " + name + " " + minY + " " + height + " " + sky + " " + g0 + " " + n);
            World w = new World(minY, height);
            LevelHeightAccessor acc = LevelHeightAccessor.create(minY, height);
            List<ProtoChunk> cs = new ArrayList<>();
            for (int cz = g0; cz < g0 + n; cz++) for (int cx = g0; cx < g0 + n; cx++) {
                ProtoChunk c = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, acc, f, null);
                w.chunks.put(ChunkPos.pack(cx, cz), c);
                cs.add(c);
            }
            generate(w, style, g0, n, r);
            sections(w, g0, n);
            LevelLightEngine engine = new LevelLightEngine(w, true, sky);
            for (ProtoChunk c : cs) { c.initializeLightSources(); c.setLightEngine(engine); c.setPersistedStatus(ChunkStatus.LIGHT); }
            out.println("INIT");
            initLight(w, engine, cs, false);
            Random q = new Random(r.nextLong());
            snapshot(w, engine, "init", q);
            int lo = g0 * 16, hi = (g0 + n) * 16 - 1, top = minY + height - 1;
            for (int batch = 0; batch < 8; batch++) {
                int ops = 12 + r.nextInt(40);
                for (int i = 0; i < ops; i++) {
                    int x = lo + r.nextInt(hi - lo + 1), z = lo + r.nextInt(hi - lo + 1);
                    if (r.nextInt(3) == 0) { x = (x & ~15) + (r.nextBoolean() ? 0 : 15); }
                    if (r.nextInt(3) == 0) { z = (z & ~15) + (r.nextBoolean() ? 0 : 15); }
                    int y;
                    ProtoChunk c = w.chunks.get(ChunkPos.pack(x >> 4, z >> 4));
                    int mode = r.nextInt(10);
                    if (mode < 5) y = Math.max(minY, Math.min(top, c.getHeight(net.minecraft.world.level.levelgen.Heightmap.Types.WORLD_SURFACE, x & 15, z & 15) + r.nextInt(9) - 4));
                    else if (mode < 7) y = minY + r.nextInt(height);
                    else if (mode < 8) y = Math.min(top, minY + height - 1 - r.nextInt(6));
                    else y = Math.max(minY, Math.min(top, (minY + r.nextInt(height)) & ~15 | (r.nextBoolean() ? 0 : 15)));
                    int kind = r.nextInt(10);
                    BlockState s = kind < 3 ? Blocks.AIR.defaultBlockState() : kind < 4 ? st("cave_air") : kind < 6 ? anyState(LIGHTS[r.nextInt(LIGHTS.length)], r) : kind < 7 ? st("stone") : kind < 8 ? anyState(SHAPES[r.nextInt(SHAPES.length)], r) : ALL_STATES.get(r.nextInt(ALL_STATES.size()));
                    c.setBlockState(new BlockPos(x, y, z), s, 0);
                    out.println("OP " + x + " " + y + " " + z + " " + Block.BLOCK_STATE_REGISTRY.getId(s));
                }
                out.println("RUN");
                run(w, engine);
                snapshot(w, engine, "batch" + batch, q);
                if (batch == 3 || batch == 6) {
                    // unload + reload one chunk (random), keeping its saved light
                    int cx = g0 + r.nextInt(n), cz = g0 + r.nextInt(n);
                    ProtoChunk c = w.chunks.get(ChunkPos.pack(cx, cz));
                    ChunkPos pos = c.getPos();
                    Map<Integer, DataLayer[]> saved = new TreeMap<>();
                    for (int sy = engine.getMinLightSection(); sy < engine.getMaxLightSection(); sy++) {
                        DataLayer b = engine.getLayerListener(LightLayer.BLOCK).getDataLayerData(SectionPos.of(pos, sy));
                        DataLayer s2 = engine.getLayerListener(LightLayer.SKY).getDataLayerData(SectionPos.of(pos, sy));
                        b = b != null && !b.isEmpty() ? b.copy() : null;
                        s2 = s2 != null && !s2.isEmpty() ? s2.copy() : null;
                        saved.put(sy, new DataLayer[]{b, s2});
                    }
                    out.println("UNLOAD " + cx + " " + cz);
                    w.chunks.remove(ChunkPos.pack(cx, cz));
                    engine.retainData(pos, false);
                    engine.setLightEnabled(pos, false);
                    for (int sy = engine.getMinLightSection(); sy < engine.getMaxLightSection(); sy++) {
                        engine.queueSectionData(LightLayer.BLOCK, SectionPos.of(pos, sy), null);
                        engine.queueSectionData(LightLayer.SKY, SectionPos.of(pos, sy), null);
                    }
                    for (int sy = acc.getMinSectionY(); sy <= acc.getMaxSectionY(); sy++) engine.updateSectionStatus(SectionPos.of(pos, sy), true);
                    run(w, engine);
                    snapshot(w, engine, "unloaded" + batch, q);
                    out.println("RELOAD " + cx + " " + cz);
                    w.chunks.put(ChunkPos.pack(cx, cz), c);
                    boolean loadedAny = false;
                    for (Map.Entry<Integer, DataLayer[]> e : saved.entrySet()) {
                        DataLayer[] d = e.getValue();
                        boolean hasSky = sky && d[1] != null;
                        if (d[0] != null || hasSky) {
                            if (!loadedAny) { engine.retainData(pos, true); loadedAny = true; }
                            if (d[0] != null) engine.queueSectionData(LightLayer.BLOCK, SectionPos.of(pos, e.getKey()), d[0]);
                            if (hasSky) engine.queueSectionData(LightLayer.SKY, SectionPos.of(pos, e.getKey()), d[1]);
                        }
                    }
                    c.initializeLightSources();
                    for (int i = 0; i < c.getSectionsCount(); i++) if (!c.getSection(i).hasOnlyAir()) engine.updateSectionStatus(SectionPos.of(pos, c.getSectionYFromSectionIndex(i)), false);
                    run(w, engine);
                    engine.setLightEnabled(pos, true);
                    engine.retainData(pos, false);
                    run(w, engine);
                    snapshot(w, engine, "reloaded" + batch, q);
                }
            }
        }
        out.println("END");
        out.close();
    }
}
