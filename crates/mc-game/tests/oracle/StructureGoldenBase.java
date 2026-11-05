import java.io.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;

/**
 * Shared part of the structure golden oracles (NetherFortressOracle, MineshaftOracle, StrongholdOracle; Rust mirror:
 * crates/mc-game/tests/common/structure_golden.rs). Output per start: {@code START seed chunkX chunkZ pieceCount}, one
 * {@code P <piece tag SNBT>} line per piece ({@code StructurePiece.createTag}), then for every placement {@code PLACE cx cz} (centre
 * chunk of a 3x3 FEATURES region over the synthetic terrain; {@code placeInChunk} with the writable area of the centre chunk, the way
 * ChunkGenerator.applyBiomeDecoration does it) and the region dump as {@code D <line>} (the per-chunk {@code B} block lines are
 * folded into one {@code B* cx cz count fnv} digest line unless the environment variable NOFOLD is set), then {@code END}.
 */
public class StructureGoldenBase {
    static final int UNDERGROUND_DECORATION = 6;
    static final boolean NOFOLD = System.getenv("NOFOLD") != null;

    /** Folds the B lines of the dump per chunk into one digest line (mirrored in the Rust test). */
    static List<String> fold(List<String> lines) {
        if (NOFOLD) return lines;
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

    static StructurePieceSerializationContext pieceContext() {
        return new StructurePieceSerializationContext(null, FeatureOracleBase.ACCESS, null);
    }

    static Holder<Structure> structure(String name) {
        return FeatureOracleBase.ACCESS.lookupOrThrow(Registries.STRUCTURE).getOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.withDefaultNamespace(name)));
    }

    /** {@code Structure.generate} over the real overworld generator/biome source (the structures here do not look at biomes). */
    static StructureStart generate(Holder<Structure> structure, ResourceKey<Level> dimension, NoiseBasedChunkGenerator generator, long seed, int cx, int cz) {
        RandomState randomState = FeatureOracleBase.randomState(seed);
        return structure.value().generate(
            structure, dimension, FeatureOracleBase.ACCESS, generator, generator.getBiomeSource(),
            randomState.createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.builder().enableCaches().build()), randomState, null, seed,
            new ChunkPos(cx, cz), 0, LevelHeightAccessor.create(FeatureOracleBase.MIN_Y, FeatureOracleBase.HEIGHT), b -> true
        );
    }

    static void writeStart(PrintWriter out, StructureStart start, long seed, int cx, int cz) {
        List<StructurePiece> pieces = start.getPieces();
        out.println("START " + seed + " " + cx + " " + cz + " " + pieces.size());
        for (StructurePiece p : pieces) out.println("P " + p.createTag(pieceContext()));
    }

    /** The chunk of a world position, packed. */
    static long chunkOf(int x, int z) {
        return ChunkPos.pack(x >> 4, z >> 4);
    }

    /** The chunk containing a random block of the piece's bounding box. */
    static long randomChunkIn(StructurePiece p, Random rnd) {
        BoundingBox bb = p.getBoundingBox();
        return chunkOf(bb.minX() + rnd.nextInt(bb.getXSpan()), bb.minZ() + rnd.nextInt(bb.getZSpan()));
    }

    /** One placement (see the class comment). */
    static void place(PrintWriter out, StructureStart start, NoiseBasedChunkGenerator generator, long seed, long packedChunk) throws Exception {
        int px = ChunkPos.getX(packedChunk), pz = ChunkPos.getZ(packedChunk);
        ChunkPos chunkPos = new ChunkPos(px, pz);
        FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, px, pz, ChunkStatus.FEATURES, 1);
        FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
        WorldgenRandom random = new WorldgenRandom(new LegacyRandomSource(0L));
        long decorationSeed = random.setDecorationSeed(seed, chunkPos.getMinBlockX(), chunkPos.getMinBlockZ());
        random.setFeatureSeed(decorationSeed, 0, UNDERGROUND_DECORATION);
        BoundingBox writable = new BoundingBox(
            chunkPos.getMinBlockX(), FeatureOracleBase.MIN_Y + 1, chunkPos.getMinBlockZ(), chunkPos.getMinBlockX() + 15,
            FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1, chunkPos.getMinBlockZ() + 15
        );
        start.placeInChunk(region, null, generator, random, writable, chunkPos);
        out.println("PLACE " + px + " " + pz);
        for (String line : fold(FeatureOracleBase.dump(region, before))) out.println("D " + line);
        out.println("END");
    }
}
