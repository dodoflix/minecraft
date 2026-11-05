import java.io.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;

/**
 * Oracle of crates/mc-game/tests/golden/nether_fortress_golden.rs: the real vanilla `minecraft:fortress` structure (output format:
 * StructureGoldenBase.java). Usage (repo root, FeatureOracleBase.java and StructureGoldenBase.java next to this file):
 * scripts/vanilla-java.sh crates/mc-game/tests/oracle/NetherFortressOracle.java crates/mc-game/tests/fixtures/nether_fortress.txt
 */
public class NetherFortressOracle {
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
        return StructureGoldenBase.chunkOf(wx, wz);
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Holder<Structure> fortress = StructureGoldenBase.structure("fortress");
        NoiseBasedChunkGenerator generator = FeatureOracleBase.generator();
        Random rnd = new Random(0xF027L);
        int startsWithPlacement = 0;
        for (int i = 0; i < 40; i++) {
            long seed = rnd.nextLong();
            int cx = i < 4 ? i - 2 : rnd.nextInt(4000) - 2000, cz = i < 4 ? 3 - i : rnd.nextInt(4000) - 2000;
            StructureStart start = StructureGoldenBase.generate(fortress, Level.NETHER, generator, seed, cx, cz);
            List<StructurePiece> pieces = start.getPieces();
            StructureGoldenBase.writeStart(out, start, seed, cx, cz);
            if (i % 4 != 0 && i >= 4) continue;
            // placement centers: the first piece, the first of each interesting piece kind, two random pieces
            LinkedHashSet<Long> centers = new LinkedHashSet<>();
            Set<String> wanted = new HashSet<>(List.of("MonsterThrone", "CastleStalkRoom", "CastleEntrance", "BridgeEndFiller", "CastleCorridorStairsPiece"));
            boolean chestFound = false;
            for (StructurePiece p : pieces) {
                String kind = p.getClass().getSimpleName();
                BoundingBox bb = p.getBoundingBox();
                boolean chest = (kind.startsWith("CastleSmallCorridor") && p.createTag(StructureGoldenBase.pieceContext()).getBooleanOr("Chest", false));
                if (centers.isEmpty() || wanted.remove(kind) || (chest && !chestFound)) {
                    if (chest) chestFound = true;
                    centers.add(StructureGoldenBase.chunkOf(bb.getCenter().getX(), bb.getCenter().getZ()));
                    if (kind.equals("MonsterThrone")) centers.add(spawnerChunk(p));
                }
            }
            for (int k = 0; k < 2; k++) centers.add(StructureGoldenBase.randomChunkIn(pieces.get(rnd.nextInt(pieces.size())), rnd));
            for (long packed : centers) {
                StructureGoldenBase.place(out, start, generator, seed, packed);
                startsWithPlacement++;
            }
        }
        out.close();
        System.err.println("placements: " + startsWithPlacement);
    }
}
