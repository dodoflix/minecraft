import java.io.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;

/**
 * Oracle of crates/mc-game/tests/golden/stronghold_golden.rs: the real vanilla `minecraft:stronghold` structure (start generation incl. the
 * retry loop until a portal room exists, and placeInChunk of every piece kind; output format: StructureGoldenBase.java).
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/stronghold.txt crates/mc-game/tests/oracle/StrongholdOracle.java crates/mc-game/tests/fixtures/stronghold.txt
 * (FeatureOracleBase.java and StructureGoldenBase.java next to this file)
 * Output: crates/mc-game/tests/fixtures/stronghold.txt
 */
public class StrongholdOracle {
    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Holder<Structure> stronghold = StructureGoldenBase.structure("stronghold");
        NoiseBasedChunkGenerator generator = FeatureOracleBase.generator();
        Random rnd = new Random(0x57204L);
        int placements = 0;
        for (int i = 0; i < 44; i++) {
            long seed = rnd.nextLong();
            int cx = i < 4 ? i - 2 : rnd.nextInt(4000) - 2000, cz = i < 4 ? 3 - i : rnd.nextInt(4000) - 2000;
            StructureStart start = StructureGoldenBase.generate(stronghold, Level.OVERWORLD, generator, seed, cx, cz);
            List<StructurePiece> pieces = start.getPieces();
            StructureGoldenBase.writeStart(out, start, seed, cx, cz);
            // placement centres: the first piece, the first of each piece kind (box centre and box corner chunk), two random pieces
            LinkedHashSet<Long> centers = new LinkedHashSet<>();
            Set<String> seen = new HashSet<>();
            for (StructurePiece p : pieces) {
                BoundingBox bb = p.getBoundingBox();
                if (centers.isEmpty() || seen.add(p.getClass().getSimpleName())) {
                    centers.add(StructureGoldenBase.chunkOf(bb.getCenter().getX(), bb.getCenter().getZ()));
                    centers.add(StructureGoldenBase.chunkOf(bb.minX(), bb.minZ()));
                }
            }
            for (int k = 0; k < 2; k++) centers.add(StructureGoldenBase.randomChunkIn(pieces.get(rnd.nextInt(pieces.size())), rnd));
            for (long packed : centers) {
                StructureGoldenBase.place(out, start, generator, seed, packed);
                placements++;
            }
        }
        out.close();
        System.err.println("placements: " + placements);
    }
}
