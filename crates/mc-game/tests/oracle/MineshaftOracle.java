import java.io.*;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;

/**
 * Oracle of crates/mc-game/tests/golden/mineshaft_golden.rs: the real vanilla `minecraft:mineshaft` and `minecraft:mineshaft_mesa`
 * structures (output format: StructureGoldenBase.java). Usage (repo root, FeatureOracleBase.java and StructureGoldenBase.java next to this file):
 * scripts/vanilla-java.sh crates/mc-game/tests/oracle/MineshaftOracle.java crates/mc-game/tests/fixtures/mineshaft.txt
 */
public class MineshaftOracle {
    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Holder<Structure> normal = StructureGoldenBase.structure("mineshaft");
        Holder<Structure> mesa = StructureGoldenBase.structure("mineshaft_mesa");
        NoiseBasedChunkGenerator generator = FeatureOracleBase.generator();
        Random rnd = new Random(0x3151L);
        int placements = 0;
        for (int i = 0; i < 48; i++) {
            long seed = rnd.nextLong();
            int cx = i < 4 ? i - 2 : rnd.nextInt(4000) - 2000, cz = i < 4 ? 3 - i : rnd.nextInt(4000) - 2000;
            StructureStart start = StructureGoldenBase.generate(i % 3 == 2 ? mesa : normal, Level.OVERWORLD, generator, seed, cx, cz);
            List<StructurePiece> pieces = start.getPieces();
            StructureGoldenBase.writeStart(out, start, seed, cx, cz);
            // placement centres: the first piece (the room), the first of each piece kind / corridor variant, two random pieces
            LinkedHashSet<Long> centers = new LinkedHashSet<>();
            Set<String> seen = new HashSet<>();
            for (StructurePiece p : pieces) {
                var tag = p.createTag(StructureGoldenBase.pieceContext());
                String kind = p.getClass().getSimpleName() + (tag.getBooleanOr("hr", false) ? "+rails" : "") + (tag.getBooleanOr("sc", false) ? "+spider" : "") + (tag.getBooleanOr("tf", false) ? "+two" : "");
                if (centers.isEmpty() || seen.add(kind)) centers.add(StructureGoldenBase.chunkOf(p.getBoundingBox().getCenter().getX(), p.getBoundingBox().getCenter().getZ()));
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
