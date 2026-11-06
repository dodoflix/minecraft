import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.levelgen.blending.BlendingData;

/**
 * Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/blending_data/golden.txt crates/mc-world/tests/fixtures/blending_data/BlendingDataGolden.java crates/mc-world/tests/fixtures/blending_data/inputs.snbt crates/mc-world/tests/fixtures/blending_data/golden.txt
 * Output: crates/mc-world/tests/fixtures/blending_data/golden.txt
 */
public class BlendingDataGolden {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        PrintWriter out = new PrintWriter(new FileWriter(args[1]));
        TagParser<Tag> parser = TagParser.create(NbtOps.INSTANCE);
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isEmpty()) continue;
            Tag tag;
            try { tag = parser.parseFully(line); } catch (Exception e) { out.println("IN\t" + line + "\tPARSEERR"); continue; }
            Optional<BlendingData.Packed> r = BlendingData.Packed.CODEC.parse(NbtOps.INSTANCE, tag).resultOrPartial(e -> {});
            if (r.isEmpty()) { out.println("IN\t" + line + "\tNONE"); continue; }
            BlendingData.Packed p = r.get();
            Tag enc = BlendingData.Packed.CODEC.encodeStart(NbtOps.INSTANCE, p).getOrThrow();
            BlendingData bd = BlendingData.unpack(p);
            BlendingData.Packed repacked = bd.pack();
            Tag enc2 = BlendingData.Packed.CODEC.encodeStart(NbtOps.INSTANCE, repacked).getOrThrow();
            out.println("IN\t" + line + "\tOK\t" + enc + "\t" + bd.getAreaWithOldGeneration().getMinY() + " " + bd.getAreaWithOldGeneration().getHeight() + "\t" + enc2);
        }
        out.close();
    }
}
