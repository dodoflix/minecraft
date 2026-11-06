import java.io.*;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Beardifier;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.PoolElementStructurePiece;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.TerrainAdjustment;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;
import net.minecraft.world.level.levelgen.structure.pools.JigsawJunction;
import net.minecraft.world.level.levelgen.structure.pools.StructureTemplatePool;
import net.minecraft.world.level.levelgen.structure.structures.JigsawStructure;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager;
import net.minecraft.world.level.storage.LevelStorageSource;

/**
 * Oracle of crates/mc-game/tests/golden/jigsaw_structures_golden.rs: every vanilla {@code minecraft:jigsaw} structure (villages, pillager
 * outpost, bastion remnant, ancient city, trail ruins, trial chambers with its pool aliases, abandoned camps) generated with the
 * real {@code Structure.generate} for several seeds/chunks, then {@code placeInChunk} over the synthetic terrain.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/jigsaw_structures.txt.gz crates/mc-game/tests/oracle/JigsawStructureOracle.java crates/mc-game/tests/fixtures/jigsaw_structures.txt.gz [startsPerStructure=6] [idFilter,...]
 * (FeatureOracleBase.java next to this file; the wrapper gzips, the program writes plain text)
 * Output: crates/mc-game/tests/fixtures/jigsaw_structures.txt.gz
 *
 * Lines per start: {@code START id seed chunkX chunkZ pieceCount}, one {@code P <canonical piece tag>} line per piece (the saved
 * {@code createTag}: pool element, junctions, rotation, ground level delta, bounding box ...), per placement chunk
 * {@code PLACE cx cz} + the region dump as {@code D <line>} (block lines folded to a digest per chunk) + {@code END}, and per
 * terrain-adaptation chunk {@code BEARD cx cz <affected box|-> rigids junctions nonZeroSamples digest} (the real
 * {@code Beardifier} built the way {@code Beardifier.forStructuresInChunk} does from the start, sampled on a grid).
 */
public class JigsawStructureOracle {
    static final Random SEEDS = new Random(0x716500L);

    /** Sorted-key canonical text of a tag (mirrored by the Rust test). */
    static String canon(Tag t) {
        switch (t) {
            case CompoundTag c -> {
                StringBuilder sb = new StringBuilder("{");
                for (String k : new TreeSet<>(c.keySet())) sb.append(sb.length() > 1 ? "," : "").append(k).append(':').append(canon(c.get(k)));
                return sb.append('}').toString();
            }
            case ListTag l -> {
                StringBuilder sb = new StringBuilder("[");
                for (int i = 0; i < l.size(); i++) sb.append(i > 0 ? "," : "").append(canon(l.get(i)));
                return sb.append(']').toString();
            }
            case StringTag s -> { return "\"" + s.value() + "\""; }
            case ByteTag b -> { return b.value() + "b"; }
            case ShortTag b -> { return b.value() + "s"; }
            case IntTag b -> { return b.value() + "i"; }
            case LongTag b -> { return b.value() + "l"; }
            case FloatTag b -> { return Float.floatToRawIntBits(b.value()) + "f"; }
            case DoubleTag b -> { return Double.doubleToRawLongBits(b.value()) + "d"; }
            case IntArrayTag a -> { return "I" + Arrays.toString(a.getAsIntArray()).replace(" ", ""); }
            case ByteArrayTag a -> { return "B" + Arrays.toString(a.getAsByteArray()).replace(" ", ""); }
            case LongArrayTag a -> { return "L" + Arrays.toString(a.getAsLongArray()).replace(" ", ""); }
            default -> throw new IllegalStateException(t.toString());
        }
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
        Path dir = Files.createTempDirectory("jigsaw-oracle");
        LevelStorageSource.LevelStorageAccess access = LevelStorageSource.createDefault(dir).createAccess("world");
        return new StructureTemplateManager(resources, access, DataFixers.getDataFixer(), BuiltInRegistries.BLOCK);
    }

    static StructureTemplateManager TEMPLATES;

    static StructurePieceSerializationContext pieceContext() {
        return new StructurePieceSerializationContext(null, FeatureOracleBase.ACCESS, TEMPLATES);
    }

    /** A {@code ServerLevel} that only knows its seed (`level.getLevel().getSeed()`, e.g. `CappedProcessor`): allocated without construction. */
    static net.minecraft.server.level.ServerLevel levelWithSeed(long seed) throws Exception {
        java.lang.reflect.Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) uf.get(null);
        net.minecraft.server.MinecraftServer server = (net.minecraft.server.MinecraftServer) unsafe.allocateInstance(net.minecraft.server.dedicated.DedicatedServer.class);
        Object settings = unsafe.allocateInstance(net.minecraft.world.level.levelgen.WorldGenSettings.class);
        set(settings, "options", new net.minecraft.world.level.levelgen.WorldOptions(seed, false, false));
        set(server, "worldGenSettings", settings);
        net.minecraft.server.level.ServerLevel level = (net.minecraft.server.level.ServerLevel) unsafe.allocateInstance(net.minecraft.server.level.ServerLevel.class);
        set(level, "server", server);
        return level;
    }

    static void set(Object o, String name, Object value) throws Exception {
        Class<?> c = o.getClass();
        while (true) {
            try {
                java.lang.reflect.Field f = c.getDeclaredField(name);
                f.setAccessible(true);
                f.set(o, value);
                return;
            } catch (NoSuchFieldException e) {
                c = c.getSuperclass();
            }
        }
    }

    /** `Beardifier.forStructuresInChunk` for a single start, then sampled on a grid. */
    static String beard(StructureStart start, ChunkPos chunkPos) {
        TerrainAdjustment adjustment = start.getStructure().terrainAdaptation();
        if (adjustment == TerrainAdjustment.NONE) return "-";
        int chunkStartBlockX = chunkPos.getMinBlockX(), chunkStartBlockZ = chunkPos.getMinBlockZ();
        List<Beardifier.Rigid> rigids = new ArrayList<>();
        List<JigsawJunction> junctions = new ArrayList<>();
        BoundingBox any = null;
        for (StructurePiece piece : start.getPieces()) {
            if (!piece.isCloseToChunk(chunkPos, 12)) continue;
            if (piece instanceof PoolElementStructurePiece poolPiece) {
                if (poolPiece.getElement().getProjection() == StructureTemplatePool.Projection.RIGID) {
                    rigids.add(new Beardifier.Rigid(poolPiece.getBoundingBox(), adjustment, poolPiece.getGroundLevelDelta()));
                    any = any == null ? piece.getBoundingBox() : BoundingBox.encapsulating(any, piece.getBoundingBox());
                }
                for (JigsawJunction junction : poolPiece.getJunctions()) {
                    int jx = junction.getSourceX(), jz = junction.getSourceZ();
                    if (jx > chunkStartBlockX - 12 && jz > chunkStartBlockZ - 12 && jx < chunkStartBlockX + 15 + 12 && jz < chunkStartBlockZ + 15 + 12) {
                        junctions.add(junction);
                        BoundingBox jb = new BoundingBox(new net.minecraft.core.BlockPos(jx, junction.getSourceGroundY(), jz));
                        any = any == null ? jb : BoundingBox.encapsulating(any, jb);
                    }
                }
            } else {
                rigids.add(new Beardifier.Rigid(piece.getBoundingBox(), adjustment, 0));
                any = any == null ? piece.getBoundingBox() : BoundingBox.encapsulating(any, piece.getBoundingBox());
            }
        }
        if (any == null) return "empty";
        BoundingBox affected = any.inflatedBy(24);
        Beardifier beardifier = new Beardifier(List.copyOf(rigids), List.copyOf(junctions), affected);
        FeatureOracleBase.Fnv fnv = new FeatureOracleBase.Fnv();
        int nonZero = 0;
        int[] xs = {0, 3, 8, 15}, zs = {0, 6, 9, 15};
        for (int xi : xs) for (int zi : zs) {
            int x = chunkStartBlockX + xi, z = chunkStartBlockZ + zi;
            for (int y = affected.minY(); y <= affected.maxY(); y += 3) {
                float v = beardifier.sampleValue(null, x, y, z);
                if (v != 0.0F) nonZero++;
                fnv.i(Float.floatToRawIntBits(v));
            }
        }
        return affected.minX() + "," + affected.minY() + "," + affected.minZ() + "," + affected.maxX() + "," + affected.maxY() + "," + affected.maxZ()
            + " " + rigids.size() + " " + junctions.size() + " " + nonZero + " " + fnv.hex();
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        int perStructure = args.length > 1 ? Integer.parseInt(args[1]) : 6;
        Set<String> filter = args.length > 2 && !args[2].isEmpty() ? new HashSet<>(Arrays.asList(args[2].split(","))) : null;
        boolean noFold = args.length > 3 && args[3].equals("nofold"); // debugging: unfolded block lines (the Rust test: JIGSAW_NOFOLD=1 JIGSAW_FIXTURE=<file>)
        TEMPLATES = templateManager();
        StructureTemplateManager templates = TEMPLATES;
        // what ReloadableServerResources.updateComponentsAndStaticRegistryTags does: items need components to load block entity inventories
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(FeatureOracleBase.ACCESS).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        NoiseBasedChunkGenerator generator = FeatureOracleBase.generator();
        var registry = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.STRUCTURE);
        List<net.minecraft.resources.Identifier> ids = new ArrayList<>();
        registry.keySet().forEach(ids::add);
        Collections.sort(ids);
        int placements = 0, starts = 0;
        for (net.minecraft.resources.Identifier id : ids) {
            Holder<Structure> holder = registry.get(id).orElseThrow();
            if (!(holder.value() instanceof JigsawStructure) || (filter != null && !filter.contains(id.getPath()))) continue;
            Random rnd = new Random(SEEDS.nextLong());
            for (int i = 0; i < perStructure; i++) {
                long seed = rnd.nextLong();
                int cx = i < 2 ? i * 3 - 1 : rnd.nextInt(4000) - 2000, cz = i < 2 ? 2 - i * 5 : rnd.nextInt(4000) - 2000;
                RandomState randomState = FeatureOracleBase.randomState(seed);
                StructureStart start = holder.value().generate(
                    holder, Level.OVERWORLD, FeatureOracleBase.ACCESS, generator, generator.getBiomeSource(),
                    randomState.createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.builder().enableCaches().build()), randomState, templates, seed,
                    new ChunkPos(cx, cz), 0, LevelHeightAccessor.create(FeatureOracleBase.MIN_Y, FeatureOracleBase.HEIGHT), b -> true
                );
                List<StructurePiece> pieces = start.isValid() ? start.getPieces() : List.of();
                out.println("START " + id + " " + seed + " " + cx + " " + cz + " " + pieces.size());
                starts++;
                for (StructurePiece p : pieces) out.println("P " + canon(p.createTag(pieceContext())));
                if (pieces.isEmpty()) continue;
                // the persisted form: the start as the chunk stores it, loaded again (e.g. the bounding box is recomputed from the element)
                List<StructurePiece> reloaded = StructureStart.loadStaticStart(pieceContext(), start.createTag(pieceContext(), new ChunkPos(cx, cz)), seed).getPieces();
                out.println("R " + reloaded.size());
                for (int k = 0; k < reloaded.size(); k++) {
                    String again = canon(reloaded.get(k).createTag(pieceContext()));
                    if (!again.equals(canon(pieces.get(k).createTag(pieceContext())))) out.println("RD " + k + " " + again);
                }
                // chunks to place / sample: the first piece's, and random points of random pieces
                LinkedHashSet<Long> centers = new LinkedHashSet<>();
                BoundingBox first = pieces.get(0).getBoundingBox();
                centers.add(ChunkPos.pack(first.getCenter().getX() >> 4, first.getCenter().getZ() >> 4));
                int extra = Math.min(pieces.size() - 1, 3);
                for (int k = 0; k < extra; k++) {
                    BoundingBox bb = pieces.get(1 + rnd.nextInt(pieces.size() - 1)).getBoundingBox();
                    centers.add(ChunkPos.pack((bb.minX() + rnd.nextInt(bb.getXSpan())) >> 4, (bb.minZ() + rnd.nextInt(bb.getZSpan())) >> 4));
                }
                for (long packed : centers) {
                    int px = ChunkPos.getX(packed), pz = ChunkPos.getZ(packed);
                    ChunkPos chunkPos = new ChunkPos(px, pz);
                    out.println("BEARD " + px + " " + pz + " " + beard(start, chunkPos));
                    FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, px, pz, ChunkStatus.FEATURES, 1);
                    set(region, "level", levelWithSeed(seed));
                    FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
                    WorldgenRandom random = new WorldgenRandom(new LegacyRandomSource(0L));
                    long decorationSeed = random.setDecorationSeed(seed, chunkPos.getMinBlockX(), chunkPos.getMinBlockZ());
                    random.setFeatureSeed(decorationSeed, 0, holder.value().step().ordinal());
                    BoundingBox writable = new BoundingBox(
                        chunkPos.getMinBlockX(), FeatureOracleBase.MIN_Y + 1, chunkPos.getMinBlockZ(), chunkPos.getMinBlockX() + 15,
                        FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1, chunkPos.getMinBlockZ() + 15
                    );
                    start.placeInChunk(region, null, generator, random, writable, chunkPos);
                    out.println("PLACE " + px + " " + pz);
                    List<String> dumped = FeatureOracleBase.dump(region, before);
                    for (String line : noFold ? dumped : fold(dumped)) out.println("D " + line);
                    out.println("END");
                    placements++;
                }
            }
            out.flush();
        }
        out.close();
        System.err.println("starts: " + starts + ", placements: " + placements);
    }
}
