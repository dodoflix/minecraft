import java.io.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
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
import net.minecraft.world.level.levelgen.structure.BuiltinStructures;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;

/**
 * Oracle of crates/mc-game/tests/nether_fortress_golden.rs: the real vanilla `minecraft:fortress` structure.
 * Usage (repo root, FeatureOracleBase.java next to this file):
 * scripts/vanilla-java.sh crates/mc-game/tests/oracle/NetherFortressOracle.java crates/mc-game/tests/fixtures/nether_fortress.txt
 *
 * Output per start: {@code START seed chunkX chunkZ pieceCount}, one {@code P <piece tag SNBT>} line per piece (StructurePiece.createTag),
 * then for every placement {@code PLACE cx cz} (center chunk of a 3x3 FEATURES region over the synthetic terrain, placeInChunk with the
 * writable area of the center chunk, the way ChunkGenerator.applyBiomeDecoration does it) and the region dump as {@code D <line>}
 * (the per-chunk {@code B} block lines are folded into a {@code B* cx cz count fnv} digest), then {@code END}.
 */
public class NetherFortressOracle {
    static final int UNDERGROUND_DECORATION = 6;

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

    /** Chunk of the piece-local position (3, y, 5) (the throne's spawner), `StructurePiece.getWorldX/Z` by hand (protected). */
    static long spawnerChunk(StructurePiece p) {
        BoundingBox bb = p.getBoundingBox();
        int x = 3, z = 5, wx, wz;
        switch (p.getOrientation()) {
            case WEST -> { wx = bb.maxX() - z; wz = bb.minZ() + x; }
            case EAST -> { wx = bb.minX() + z; wz = bb.minZ() + x; }
            case NORTH -> { wx = bb.minX() + x; wz = bb.maxZ() - z; }
            default -> { wx = bb.minX() + x; wz = bb.minZ() + z; }
        }
        return ChunkPos.pack(wx >> 4, wz >> 4);
    }

    static StructurePieceSerializationContext pieceContext() {
        return new StructurePieceSerializationContext(null, FeatureOracleBase.ACCESS, null);
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Holder<Structure> fortress = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.STRUCTURE).getOrThrow(BuiltinStructures.FORTRESS);
        NoiseBasedChunkGenerator generator = FeatureOracleBase.generator();
        Random rnd = new Random(0xF027L);
        int startsWithPlacement = 0;
        for (int i = 0; i < 40; i++) {
            long seed = rnd.nextLong();
            int cx = i < 4 ? i - 2 : rnd.nextInt(4000) - 2000, cz = i < 4 ? 3 - i : rnd.nextInt(4000) - 2000;
            RandomState randomState = FeatureOracleBase.randomState(seed);
            StructureStart start = fortress.value().generate(
                fortress, Level.NETHER, FeatureOracleBase.ACCESS, generator, generator.getBiomeSource(), randomState.createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.builder().enableCaches().build()), randomState, null, seed,
                new ChunkPos(cx, cz), 0, LevelHeightAccessor.create(FeatureOracleBase.MIN_Y, FeatureOracleBase.HEIGHT), b -> true
            );
            List<StructurePiece> pieces = start.getPieces();
            out.println("START " + seed + " " + cx + " " + cz + " " + pieces.size());
            for (StructurePiece p : pieces) out.println("P " + p.createTag(pieceContext()));
            if (i % 4 != 0 && i >= 4) continue;
            // placement centers: the first piece, the first of each interesting piece kind, two random pieces
            LinkedHashSet<Long> centers = new LinkedHashSet<>();
            Set<String> wanted = new HashSet<>(List.of("MonsterThrone", "CastleStalkRoom", "CastleEntrance", "BridgeEndFiller", "CastleCorridorStairsPiece"));
            boolean chestFound = false;
            for (StructurePiece p : pieces) {
                String kind = p.getClass().getSimpleName();
                BoundingBox bb = p.getBoundingBox();
                boolean chest = (kind.startsWith("CastleSmallCorridor") && p.createTag(pieceContext()).getBooleanOr("Chest", false));
                if (centers.isEmpty() || wanted.remove(kind) || (chest && !chestFound)) {
                    if (chest) chestFound = true;
                    centers.add(ChunkPos.pack(bb.getCenter().getX() >> 4, bb.getCenter().getZ() >> 4));
                    if (kind.equals("MonsterThrone")) centers.add(spawnerChunk(p));
                }
            }
            for (int k = 0; k < 2; k++) {
                BoundingBox bb = pieces.get(rnd.nextInt(pieces.size())).getBoundingBox();
                centers.add(ChunkPos.pack((bb.minX() + rnd.nextInt(bb.getXSpan())) >> 4, (bb.minZ() + rnd.nextInt(bb.getZSpan())) >> 4));
            }
            for (long packed : centers) {
                int px = ChunkPos.getX(packed), pz = ChunkPos.getZ(packed);
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
                startsWithPlacement++;
            }
        }
        out.close();
        System.err.println("placements: " + startsWithPlacement);
    }
}
