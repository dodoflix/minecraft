import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.TagParser;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager;
import net.minecraft.world.level.storage.LevelStorageSource;

/**
 * Oracle of crates/mc-game/tests/golden/scattered_structures_golden.rs: a real vanilla structure (`Structure.generate`: every piece's
 * saved tag), then `StructureStart.placeInChunk` over the synthetic terrain of FeatureOracleBase for every chunk of the start's
 * bounding box (the way ChunkGenerator.applyBiomeDecoration does it) with the region dump incl. block entity NBT and entities.
 * Usage (repo root, FeatureOracleBase.java next to this file):
 * scripts/vanilla-java.sh crates/mc-game/tests/oracle/ScatteredStructuresOracle.java out.txt STRUCTURE_ID COUNT [nether] [seed] [fullBlockLines]
 *
 * Output per start: {@code START id seed chunkX chunkZ pieceCount} (pieceCount -1: no valid start), one {@code P <piece tag SNBT>} per
 * piece, then per placement {@code PLACE cx cz featureIndex} + {@code D <dump line>} (block lines folded to a {@code B* cx cz count fnv} digest
 * unless fullBlockLines) + {@code END}. Mobs spawned by pieces (drowned, witch, cat) are created through a fake ServerLevel;
 * their {@code finalizeSpawn} randomness (region random, unseeded Entity.random, UUID) is not reproducible, so the entity lines
 * carry the tag without the keys it influences.
 */
public class ScatteredStructuresOracle {
    /** Neutral outcome of every roll a mob's finalizeSpawn makes (no equipment, no baby, not left-handed). */
    static class ConstRandom extends LegacyRandomSource {
        ConstRandom() { super(0L); }
        @Override public float nextFloat() { return 0.5F; }
        @Override public double nextDouble() { return 0.5; }
        @Override public int nextInt(int bound) { return bound / 2; }
        @Override public boolean nextBoolean() { return false; }
        @Override public long nextLong() { return 0L; }
    }

    public static class FakeLevel extends net.minecraft.server.level.ServerLevel {
        FakeLevel() { super(null, null, null, null, null, null, false, 0L, null, false); }
        public net.minecraft.server.MinecraftServer srv;
        public net.minecraft.world.level.StructureManager structureManager;
        public long seed;
        int ids = 100;
        @Override public long getSeed() { return seed; }
        @Override public long getGameTime() { return 0L; }
        public net.minecraft.world.attribute.EnvironmentAttributeSystem attrs;
        @Override public net.minecraft.world.attribute.EnvironmentAttributeSystem environmentAttributes() { return attrs; }
        @Override public net.minecraft.world.flag.FeatureFlagSet enabledFeatures() { return net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS; }
        @Override public net.minecraft.world.Difficulty getDifficulty() { return net.minecraft.world.Difficulty.NORMAL; }
        @Override public net.minecraft.server.MinecraftServer getServer() { return srv; }
        @Override public int getNextEntityId() { return ids++; }
        @Override public int getSeaLevel() { return FeatureOracleBase.SEA_LEVEL; }
        @Override public net.minecraft.core.RegistryAccess registryAccess() { return FeatureOracleBase.ACCESS; }
        @Override public net.minecraft.world.level.StructureManager structureManager() { return structureManager; }
    }

    /** StructureManager.getStructureWithPieceAt without a level: no structure around. */
    public static class NoStructures extends net.minecraft.world.level.StructureManager {
        NoStructures() { super(null, null, null); }
        @Override public StructureStart getStructureWithPieceAt(net.minecraft.core.BlockPos pos, java.util.function.Predicate<Holder<Structure>> predicate) { return StructureStart.INVALID_START; }
    }

    static void setField(Class<?> c, Object o, String name, Object v) throws Exception {
        java.lang.reflect.Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        f.set(o, v);
    }

    static sun.misc.Unsafe UNSAFE;

    static FakeLevel fakeLevel() throws Exception {
        java.lang.reflect.Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        UNSAFE = (sun.misc.Unsafe) uf.get(null);
        FakeLevel level = (FakeLevel) UNSAFE.allocateInstance(FakeLevel.class);
        level.srv = (net.minecraft.server.MinecraftServer) UNSAFE.allocateInstance(net.minecraft.server.dedicated.DedicatedServer.class);
        setField(net.minecraft.server.MinecraftServer.class, level.srv, "debugSubscribers", new net.minecraft.util.debug.ServerDebugSubscribers(level.srv));
        setField(net.minecraft.world.level.Level.class, level, "random", RandomSource.create());
        level.attrs = net.minecraft.world.attribute.EnvironmentAttributeSystem.builder().build();
        level.structureManager = (net.minecraft.world.level.StructureManager) UNSAFE.allocateInstance(NoStructures.class);
        return level;
    }

    /** Keys of a mob tag that finalizeSpawn / the unseeded entity random / the UUID decide. */
    static final String[] VOLATILE = {"UUID", "attributes", "LeftHanded", "CanPickUpLoot", "CanBreakDoors", "IsBaby", "equipment", "drop_chances", "variant", "sound_variant", "Age", "ForcedAge"};

    /**
     * The entities of the igloo templates (villager, zombie villager) are loaded and saved by their real classes (data fixer
     * formats, brain, gossips, ...), which Rust has no mob classes for: only these keys are compared for them.
     */
    static final java.util.Set<String> TEMPLATE_ENTITY_KEYS = java.util.Set.of("id", "Pos", "Rotation");

    static String normalizeEntity(String line, boolean templateEntitiesOnlyBasics) throws Exception {
        String[] t = line.split(" ", 4);
        CompoundTag tag = TagParser.parseCompoundFully(t[3]);
        if (templateEntitiesOnlyBasics) for (String k : new ArrayList<>(tag.keySet())) if (!TEMPLATE_ENTITY_KEYS.contains(k)) tag.remove(k);
        for (String k : VOLATILE) tag.remove(k);
        return t[0] + " " + t[1] + " " + t[2] + " " + tag;
    }

    /** Folds the B lines of the dump per chunk into one digest line (mirrored in the Rust test). */
    static List<String> fold(List<String> lines) {
        List<String> out = new ArrayList<>();
        long cx = 0, cz = 0, count = 0;
        FeatureOracleBase.Fnv fnv = null;
        for (String line : lines) {
            if (line.startsWith("B ")) {
                String[] t = line.split(" ");
                long x = Math.floorDiv(Integer.parseInt(t[1]), 16), z = Math.floorDiv(Integer.parseInt(t[3]), 16);
                if (fnv != null && (x != cx || z != cz)) {
                    out.add("B* " + cx + " " + cz + " " + count + " " + fnv.hex());
                    fnv = null;
                }
                if (fnv == null) {
                    fnv = new FeatureOracleBase.Fnv();
                    cx = x; cz = z; count = 0;
                }
                fnv.s(line);
                count++;
            } else {
                if (fnv != null) {
                    out.add("B* " + cx + " " + cz + " " + count + " " + fnv.hex());
                    fnv = null;
                }
                out.add(line);
            }
        }
        if (fnv != null) out.add("B* " + cx + " " + cz + " " + count + " " + fnv.hex());
        return out;
    }

    static StructureTemplateManager templateManager() throws Exception {
        MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources()));
        Path dir = Files.createTempDirectory("scattered-oracle");
        LevelStorageSource.LevelStorageAccess access = LevelStorageSource.createDefault(dir).createAccess("world");
        return new StructureTemplateManager(resources, access, net.minecraft.util.datafix.DataFixers.getDataFixer(), BuiltInRegistries.BLOCK);
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        String out = args[0], id = args[1];
        int count = Integer.parseInt(args[2]);
        boolean nether = args.length > 3 && args[3].equals("nether");
        long baseSeed = args.length > 4 ? Long.parseLong(args[4]) : 0x5CA77EL;
        boolean full = args.length > 5 && args[5].equals("full");
        PrintWriter w = new PrintWriter(new FileWriter(out));
        Holder<Structure> structure = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.STRUCTURE).getOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.withDefaultNamespace(id)));
        NoiseBasedChunkGenerator generator = nether
            ? new NoiseBasedChunkGenerator(
                MultiNoiseBiomeSource.createFromPreset(FeatureOracleBase.ACCESS.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST).getOrThrow(MultiNoiseBiomeSourceParameterLists.NETHER)),
                FeatureOracleBase.ACCESS.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.NETHER))
            : FeatureOracleBase.generator();
        StructureTemplateManager templates = templateManager();
        FakeLevel fakeLevel = fakeLevel();
        StructurePieceSerializationContext pieceContext = new StructurePieceSerializationContext(null, FeatureOracleBase.ACCESS, templates);
        Random rnd = new Random(baseSeed ^ id.hashCode());
        int placements = 0, valid = 0;
        for (int i = 0; i < count; i++) {
            long seed = rnd.nextLong();
            int cx = rnd.nextInt(4000) - 2000, cz = rnd.nextInt(4000) - 2000;
            RandomState randomState = nether
                ? RandomState.create(FeatureOracleBase.ACCESS.lookupOrThrow(Registries.NOISE), seed, FeatureOracleBase.ACCESS.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.NETHER).value())
                : FeatureOracleBase.randomState(seed);
            StructureStart start = structure.value().generate(
                structure, nether ? Level.NETHER : Level.OVERWORLD, FeatureOracleBase.ACCESS, generator, generator.getBiomeSource(),
                randomState.createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.builder().enableCaches().build()), randomState, templates, seed,
                new ChunkPos(cx, cz), 0, LevelHeightAccessor.create(FeatureOracleBase.MIN_Y, FeatureOracleBase.HEIGHT), b -> true
            );
            if (!start.isValid()) {
                w.println("START " + id + " " + seed + " " + cx + " " + cz + " -1");
                continue;
            }
            valid++;
            List<StructurePiece> pieces = start.getPieces();
            w.println("START " + id + " " + seed + " " + cx + " " + cz + " " + pieces.size());
            for (StructurePiece p : pieces) w.println("P " + p.createTag(pieceContext));
            BoundingBox bb = start.getBoundingBox();
            List<long[]> centers = new ArrayList<>();
            for (int px = bb.minX() >> 4; px <= bb.maxX() >> 4; px++)
                for (int pz = bb.minZ() >> 4; pz <= bb.maxZ() >> 4; pz++) centers.add(new long[]{px, pz});
            if (centers.size() > 9) centers = centers.subList(0, 9);
            // every third start is placed in all its chunks, the others in one random chunk (all variants still get placed)
            int pick = rnd.nextInt(centers.size());
            if (i % 3 != 0) centers = List.of(centers.get(pick));
            int index = i % 3 != 0 ? rnd.nextInt(5) : 0;
            for (long[] c : centers) {
                int px = (int) c[0], pz = (int) c[1];
                ChunkPos chunkPos = new ChunkPos(px, pz);
                FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, px, pz, ChunkStatus.FEATURES, 1);
                fakeLevel.seed = seed;
                region.fakeLevel = fakeLevel;
                region.finalizeSpawnRandom = new ConstRandom();
                FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
                WorldgenRandom random = new WorldgenRandom(new LegacyRandomSource(0L));
                long decorationSeed = random.setDecorationSeed(seed, chunkPos.getMinBlockX(), chunkPos.getMinBlockZ());
                random.setFeatureSeed(decorationSeed, index, structure.value().step().ordinal());
                BoundingBox writable = new BoundingBox(
                    chunkPos.getMinBlockX(), FeatureOracleBase.MIN_Y + 1, chunkPos.getMinBlockZ(), chunkPos.getMinBlockX() + 15,
                    FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1, chunkPos.getMinBlockZ() + 15
                );
                start.placeInChunk(region, null, generator, random, writable, chunkPos);
                w.println("PLACE " + px + " " + pz + " " + index);
                index++;
                List<String> dump = FeatureOracleBase.dump(region, before);
                if (!full) dump = fold(dump);
                for (String line : dump) w.println("D " + (line.startsWith("N ") ? normalizeEntity(line, id.equals("igloo")) : line));
                w.println("END");
                placements++;
            }
        }
        w.close();
        System.err.println(id + ": " + valid + "/" + count + " valid starts, " + placements + " placements");
    }
}
