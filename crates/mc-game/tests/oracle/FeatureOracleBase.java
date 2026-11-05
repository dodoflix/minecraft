import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.level.WorldGenRegion;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.util.RandomSource;
import net.minecraft.util.StaticCache2D;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.BiomeResolver;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.chunk.status.ChunkPyramid;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.chunk.status.ChunkStep;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.SavedTick;
import net.minecraft.world.ticks.WorldGenTickAccess;

/**
 * Shared harness of the worldgen-feature golden tests (Rust mirror: crates/mc-game/tests/common/feature_harness.rs).
 *
 * <p>How a feature oracle uses it (see GenLevelOracle.java; run with
 * {@code scripts/vanilla-java.sh crates/mc-game/tests/oracle/GenLevelOracle.java <out>}; JDK multi-file launch finds this class
 * next to the oracle):
 * <pre>
 *   FeatureOracleBase.setup();                                   // vanilla registries incl. data-pack registries
 *   Region r = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);   // 5x5 synthetic chunks, step FEATURES
 *   Snapshot before = FeatureOracleBase.snapshot(r);
 *   ... run the vanilla class under test with r (a real WorldGenRegion subclass) and r.random / generator() ...
 *   List&lt;String&gt; lines = FeatureOracleBase.dump(r, before);       // compact text dump compared line by line by Rust
 * </pre>
 * The synthetic terrain ({@link #terrain}) is a pure integer formula of (seed, x, y, z) mirrored in the Rust harness and
 * checked by the {@code T}/{@code M} digest lines of {@link #dump}. Chunks have status TERRAIN and all heightmaps
 * primed (the state of the neighbourhood during the FEATURES step; the two WG ones optionally). The region is vanilla's {@code WorldGenRegion} allocated
 * with Unsafe (no ServerLevel); only methods that dereference the (null) level are overridden: {@code setBlock} (vanilla body
 * minus the POI hook), height, sea level, registry access, light (always 0) and enabled features.
 */
public class FeatureOracleBase {
    public static RegistryAccess.Frozen ACCESS;
    public static PalettedContainerFactory FACTORY;
    public static Registry<Biome> BIOMES;
    public static final int MIN_Y = -64, HEIGHT = 384, SEA_LEVEL = 63;
    public static final String[] BIOME_NAMES = {"plains", "forest", "desert", "ocean", "river", "swamp", "taiga", "birch_forest"};
    static final StringBuilder NOTHING = new StringBuilder();

    // ---- deterministic hash helpers (ported verbatim in the Rust harness) ----
    public static long mix64(long z) {
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return z ^ (z >>> 31);
    }

    public static long h(long seed, long a, long b, long c) {
        return mix64(seed * 0x9E3779B97F4A7C15L ^ mix64(a * 0xD6E8FEB86659FD93L ^ mix64(b * 0xCA5A826395121157L ^ mix64(c + 0x1234567L))));
    }

    public static int hmod(long v, int m) {
        return (int) ((v >>> 1) % m);
    }

    /** FNV-1a 64 over bytes/ints/longs/strings. */
    public static final class Fnv {
        public long v = 0xcbf29ce484222325L;
        public long count = 0;

        public void b(int x) {
            v ^= (x & 0xFF);
            v *= 0x100000001b3L;
        }

        public void i(int x) {
            b(x); b(x >>> 8); b(x >>> 16); b(x >>> 24);
            count++;
        }

        public void l(long x) {
            i((int) x); i((int) (x >>> 32));
        }

        public void s(String s) {
            for (byte c : s.getBytes(java.nio.charset.StandardCharsets.UTF_8)) b(c);
            b(0xFF);
            count++;
        }

        public String hex() {
            return Long.toHexString(v);
        }
    }

    // ---- setup ----
    public static void setup() throws Exception {
        if (ACCESS != null) return;
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        java.util.concurrent.Executor direct = Runnable::run;
        MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources()));
        LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
        List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
        staticTags.forEach(Registry.PendingTags::apply);
        var worldContext = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
        var world = RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, direct).join();
        List<HolderLookup.RegistryLookup<?>> dimContext = new ArrayList<>(worldContext);
        world.listRegistries().forEach(dimContext::add);
        var dims = RegistryDataLoader.load(resources, dimContext, RegistryDataLoader.DIMENSION_REGISTRIES, direct).join();
        ACCESS = layers.replaceFrom(RegistryLayer.WORLD, world, dims).compositeAccess();
        BIOMES = ACCESS.lookupOrThrow(Registries.BIOME);
        FACTORY = PalettedContainerFactory.create(ACCESS);
    }

    // ---- generator / random state (the real overworld ones; the terrain of the chunks is synthetic) ----
    static NoiseBasedChunkGenerator GENERATOR;

    public static NoiseBasedChunkGenerator generator() {
        if (GENERATOR == null) {
            var source = MultiNoiseBiomeSource.createFromPreset(ACCESS.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST).getOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD));
            GENERATOR = new NoiseBasedChunkGenerator(source, ACCESS.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD));
        }
        return GENERATOR;
    }

    public static RandomState randomState(long seed) {
        NoiseGeneratorSettings settings = ACCESS.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD).value();
        return RandomState.create(ACCESS.lookupOrThrow(Registries.NOISE), seed, settings);
    }

    // ---- synthetic terrain ----
    static int lattice(long seed, int i, int j) {
        return hmod(h(seed, i, j, 1), 24);
    }

    /** First y above the solid surface of column (x, z): 50..75. */
    public static int surfaceTop(long seed, int x, int z) {
        int gx = Math.floorDiv(x, 8), gz = Math.floorDiv(z, 8), fx = Math.floorMod(x, 8), fz = Math.floorMod(z, 8);
        int a = lattice(seed, gx, gz), b = lattice(seed, gx + 1, gz), c = lattice(seed, gx, gz + 1), d = lattice(seed, gx + 1, gz + 1);
        int v = ((a * (8 - fx) + b * fx) * (8 - fz) + (c * (8 - fx) + d * fx) * fz) / 64;
        return 50 + v + (hmod(h(seed, x, z, 2), 3) == 0 ? 1 : 0);
    }

    public static BlockState terrain(long seed, int x, int y, int z) {
        if (y == MIN_Y) return Blocks.BEDROCK.defaultBlockState();
        if (y < MIN_Y + 5 && hmod(h(seed, x, y, z), 5) >= y - MIN_Y) return Blocks.BEDROCK.defaultBlockState();
        int top = surfaceTop(seed, x, z);
        if (y >= top) return (y < SEA_LEVEL ? Blocks.WATER : Blocks.AIR).defaultBlockState();
        Block bed = hmod(h(seed, x >> 3, z >> 3, 3), 2) == 0 ? Blocks.SAND : Blocks.GRAVEL;
        if (y == top - 1) return (top <= 64 ? bed : Blocks.GRASS_BLOCK).defaultBlockState();
        if (y >= top - 3) return (top <= 64 ? bed : Blocks.DIRT).defaultBlockState();
        boolean cave = y > MIN_Y + 5 && y < top - 8 && hmod(h(seed, x >> 2, y >> 2, z >> 2), 9) == 0;
        if (cave) return (y < -50 ? Blocks.LAVA : Blocks.AIR).defaultBlockState();
        return (y < hmod(h(seed, x >> 2, 0, z >> 2), 8) ? Blocks.DEEPSLATE : Blocks.STONE).defaultBlockState();
    }

    /** Biome of a 4x4x4 cell: 8 surface biomes in 8x8 columns, cave biomes below y=-16. */
    public static String biomeName(long seed, int qx, int qy, int qz) {
        if (qy < -4) return hmod(h(seed ^ 0xB10, qx >> 1, qy >> 1, qz >> 1), 2) == 0 ? "lush_caves" : "dripstone_caves";
        return BIOME_NAMES[hmod(h(seed ^ 0xB10, qx >> 1, 0, qz >> 1), BIOME_NAMES.length)];
    }

    static Holder<Biome> biomeHolder(String name) {
        return BIOMES.getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.BIOME, Identifier.withDefaultNamespace(name)));
    }

    /** {@code primeWg}: also the two worldgen heightmaps (present after real terrain generation; vanilla primes them lazily otherwise). */
    public static ProtoChunk makeChunk(long seed, int cx, int cz, ChunkStatus status, boolean primeWg) {
        ProtoChunk chunk = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, LevelHeightAccessor.create(MIN_Y, HEIGHT), FACTORY, null);
        for (int si = 0; si < chunk.getSectionsCount(); si++) {
            LevelChunkSection section = chunk.getSection(si);
            int baseY = MIN_Y + si * 16;
            for (int y = 0; y < 16; y++)
                for (int z = 0; z < 16; z++)
                    for (int x = 0; x < 16; x++) {
                        BlockState state = terrain(seed, cx * 16 + x, baseY + y, cz * 16 + z);
                        if (!state.isAir()) section.setBlockState(x, y, z, state, false);
                    }
        }
        chunk.fillBiomesFromNoise((qx, qy, qz) -> biomeHolder(biomeName(seed, qx, qy, qz)));
        chunk.setPersistedStatus(status);
        Heightmap.primeHeightmaps(chunk, ChunkStatus.FINAL_HEIGHTMAPS);
        if (primeWg) Heightmap.primeHeightmaps(chunk, EnumSet.of(Heightmap.Types.WORLD_SURFACE_WG, Heightmap.Types.OCEAN_FLOOR_WG));
        return chunk;
    }

    // ---- the region ----
    public static class Region extends WorldGenRegion {
        Region() {
            super(null, null, null, null);
        }

        public long seed;
        public long gameTime;
        public Map<Long, ProtoChunk> chunks = new HashMap<>();

        @Override public int getMinY() { return MIN_Y; }
        @Override public int getHeight() { return HEIGHT; }
        @Override public int getSeaLevel() { return SEA_LEVEL; }
        @Override public RegistryAccess registryAccess() { return ACCESS; }
        @Override public long getGameTime() { return gameTime; }
        @Override public FeatureFlagSet enabledFeatures() { return FeatureFlags.DEFAULT_FLAGS; }
        @Override public boolean isOldChunkAround(ChunkPos pos, int range) { return false; }
        @Override public int getBrightness(net.minecraft.world.level.LightLayer layer, BlockPos pos) { return 0; }
        @Override public int getRawBrightness(BlockPos pos, int darkening) { return 0; }
        @Override public boolean canSeeSky(BlockPos pos) { return false; }

        /** WorldGenRegion.setBlock without the POI hook (the level is null) and with the chunk-type branch of proto chunks. */
        @Override
        public boolean setBlock(final BlockPos pos, final BlockState blockState, final int updateFlags, final int updateLimit) {
            if (!this.ensureCanWrite(pos)) {
                return false;
            } else {
                ChunkAccess chunk = this.getChunk(pos);
                BlockState oldState = chunk.setBlockState(pos, blockState, updateFlags);
                if (blockState.hasBlockEntity()) {
                    CompoundTag tag = new CompoundTag();
                    tag.putInt("x", pos.getX());
                    tag.putInt("y", pos.getY());
                    tag.putInt("z", pos.getZ());
                    tag.putString("id", "DUMMY");
                    chunk.setBlockEntityNbt(tag);
                } else if (oldState != null && oldState.hasBlockEntity()) {
                    chunk.removeBlockEntity(pos);
                }
                if ((updateFlags & 16) == 0) {
                    BlockPos postProcessPos = blockState.getPostProcessPos(this, pos);
                    if (postProcessPos != null) {
                        this.getChunk(postProcessPos).markPosForPostProcessing(postProcessPos);
                    }
                }
                return true;
            }
        }
    }

    static void setField(Object o, String name, Object value) throws Exception {
        Field f = WorldGenRegion.class.getDeclaredField(name);
        f.setAccessible(true);
        f.set(o, value);
    }

    /**
     * A region centered on (cx, cz) for the generation step that produces {@code stepStatus} (vanilla ChunkPyramid step: read
     * radius, write radius, per-distance maximum status), over the synthetic chunks within {@code chunkRadius} (1 = 3x3,
     * 2 = 5x5). {@code region.random} is the real "worldgen_region_random" of the seed.
     */
    public static Region newRegion(long seed, int cx, int cz, ChunkStatus stepStatus, int chunkRadius) throws Exception {
        return newRegion(seed, cx, cz, stepStatus, chunkRadius, true);
    }

    public static Region newRegion(long seed, int cx, int cz, ChunkStatus stepStatus, int chunkRadius, boolean primeWg) throws Exception {
        setup();
        Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) uf.get(null);
        Region region = (Region) unsafe.allocateInstance(Region.class);
        region.seed = seed;
        region.chunks = new HashMap<>();
        for (int x = cx - chunkRadius; x <= cx + chunkRadius; x++)
            for (int z = cz - chunkRadius; z <= cz + chunkRadius; z++) region.chunks.put(ChunkPos.pack(x, z), makeChunk(seed, x, z, ChunkStatus.TERRAIN, primeWg));
        ChunkStep step = ChunkPyramid.GENERATION_PYRAMID.getStepTo(stepStatus);
        StaticCache2D<ChunkAccess> cache = StaticCache2D.create(cx, cz, chunkRadius, (x, z) -> region.chunks.get(ChunkPos.pack(x, z)));
        RandomState randomState = randomState(seed);
        setField(region, "cache", cache);
        setField(region, "center", region.chunks.get(ChunkPos.pack(cx, cz)));
        setField(region, "seed", seed);
        setField(region, "random", randomState.getOrCreateRandomFactory(Identifier.withDefaultNamespace("worldgen_region_random")).at(new ChunkPos(cx, cz).getWorldPosition()));
        setField(region, "blockTicks", new WorldGenTickAccess<Block>(pos -> region.getChunk(pos).getBlockTicks()));
        setField(region, "fluidTicks", new WorldGenTickAccess<Fluid>(pos -> region.getChunk(pos).getFluidTicks()));
        setField(region, "biomeManager", new BiomeManager(region, BiomeManager.obfuscateSeed(seed)));
        setField(region, "generatingStep", step);
        setField(region, "subTickCount", new java.util.concurrent.atomic.AtomicLong());
        setField(region, "uncachedBiomeResolver", generator().getBiomeSource().createUncachedResolver(randomState));
        setField(region, "centerChunkX", cx);
        setField(region, "centerChunkZ", cz);
        setField(region, "writeRadius", step.blockStateWriteRadius());
        return region;
    }

    // ---- dump ----
    public static final class Snapshot {
        final Map<Long, BlockState[]> blocks = new HashMap<>();
    }

    static List<ProtoChunk> sortedChunks(Region region) {
        List<ProtoChunk> list = new ArrayList<>(region.chunks.values());
        list.sort(Comparator.<ProtoChunk>comparingInt(c -> c.getPos().x()).thenComparingInt(c -> c.getPos().z()));
        return list;
    }

    static BlockState[] blocksOf(ProtoChunk chunk) {
        BlockState[] out = new BlockState[16 * HEIGHT * 16];
        BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
        int i = 0;
        for (int y = MIN_Y; y < MIN_Y + HEIGHT; y++)
            for (int z = 0; z < 16; z++)
                for (int x = 0; x < 16; x++) out[i++] = chunk.getBlockState(pos.set(chunk.getPos().getMinBlockX() + x, y, chunk.getPos().getMinBlockZ() + z));
        return out;
    }

    public static Snapshot snapshot(Region region) {
        Snapshot s = new Snapshot();
        for (ProtoChunk c : sortedChunks(region)) s.blocks.put(c.getPos().pack(), blocksOf(c));
        return s;
    }

    /**
     * Compact dump of every chunk of the region, in order, one line each:
     * <pre>
     * C cx cz status
     * T cx cz fnv count          initial block state ids (validates the Rust terrain mirror; from the snapshot)
     * M cx cz fnv count          initial biome names of all quarts
     * B x y z state              block differing from the snapshot (y, z, x ascending)
     * E x y z snbt               block entity NBT as saved (pending DUMMY or real), sorted by position
     * H cx cz TYPE fnv           heightmap raw data
     * P cx cz section shorts     post-processing marks in insertion order
     * K b|f x y z id delay prio  scheduled block/fluid ticks in insertion order
     * N cx cz snbt              entities
     * G nextLong                 next long of region.getRandom()
     * </pre>
     */
    public static List<String> dump(Region region, Snapshot before) {
        List<String> out = new ArrayList<>();
        for (ProtoChunk c : sortedChunks(region)) {
            int cx = c.getPos().x(), cz = c.getPos().z();
            BlockState[] was = before.blocks.get(c.getPos().pack());
            out.add("C " + cx + " " + cz + " " + c.getPersistedStatus().getName());
            Fnv t = new Fnv();
            for (BlockState s : was) t.i(Block.BLOCK_STATE_REGISTRY.getId(s));
            out.add("T " + cx + " " + cz + " " + t.hex() + " " + t.count);
            out.add("M " + cx + " " + cz + " " + biomeDigest(c));
            BlockState[] now = blocksOf(c);
            for (int i = 0; i < now.length; i++) {
                if (now[i] != was[i]) {
                    int x = cx * 16 + (i & 15), z = cz * 16 + ((i >> 4) & 15), y = MIN_Y + (i >> 8);
                    out.add("B " + x + " " + y + " " + z + " " + net.minecraft.commands.arguments.blocks.BlockStateParser.serialize(now[i]));
                }
            }
            List<BlockPos> bes = new ArrayList<>(c.getBlockEntitiesPos());
            bes.sort(Comparator.<BlockPos>comparingInt(BlockPos::getX).thenComparingInt(BlockPos::getY).thenComparingInt(BlockPos::getZ));
            for (BlockPos p : bes) out.add("E " + p.getX() + " " + p.getY() + " " + p.getZ() + " " + c.getBlockEntityNbtForSaving(p, ACCESS));
            List<Map.Entry<Heightmap.Types, Heightmap>> maps = new ArrayList<>(c.getHeightmaps());
            maps.sort(Comparator.comparing(e -> e.getKey().name()));
            for (var e : maps) {
                Fnv f = new Fnv();
                for (long l : e.getValue().getRawData()) f.l(l);
                out.add("H " + cx + " " + cz + " " + e.getKey().name() + " " + f.hex());
            }
            var post = c.getPostProcessing();
            for (int si = 0; si < post.length; si++) {
                if (post[si] == null || post[si].isEmpty()) continue;
                StringBuilder sb = new StringBuilder();
                for (short s : post[si]) sb.append(sb.length() == 0 ? "" : ",").append(s);
                out.add("P " + cx + " " + cz + " " + si + " " + sb);
            }
            for (SavedTick<Block> t2 : ((net.minecraft.world.ticks.ProtoChunkTicks<Block>) c.getBlockTicks()).scheduledTicks()) out.add(tickLine("b", t2.pos(), BuiltInRegistries.BLOCK.getKey(t2.type()), t2.delay(), t2.priority()));
            for (SavedTick<Fluid> t2 : ((net.minecraft.world.ticks.ProtoChunkTicks<Fluid>) c.getFluidTicks()).scheduledTicks()) out.add(tickLine("f", t2.pos(), BuiltInRegistries.FLUID.getKey(t2.type()), t2.delay(), t2.priority()));
            for (CompoundTag e : c.getEntities()) out.add("N " + cx + " " + cz + " " + e);
        }
        out.add("G " + region.getRandom().nextLong());
        return out;
    }

    static String tickLine(String kind, BlockPos p, Identifier id, int delay, net.minecraft.world.ticks.TickPriority prio) {
        return "K " + kind + " " + p.getX() + " " + p.getY() + " " + p.getZ() + " " + id + " " + delay + " " + prio.name();
    }

    static String biomeDigest(ProtoChunk c) {
        Fnv f = new Fnv();
        int qx0 = c.getPos().getMinBlockX() >> 2, qz0 = c.getPos().getMinBlockZ() >> 2;
        for (int qy = MIN_Y >> 2; qy < (MIN_Y + HEIGHT) >> 2; qy++)
            for (int qz = 0; qz < 4; qz++)
                for (int qx = 0; qx < 4; qx++) f.s(c.getNoiseBiome(qx0 + qx, qy, qz0 + qz).unwrapKey().orElseThrow().identifier().getPath());
        return f.hex() + " " + f.count;
    }
}
