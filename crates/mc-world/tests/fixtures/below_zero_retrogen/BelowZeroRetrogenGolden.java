import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.levelgen.BelowZeroRetrogen;

/** Usage (repo root): scripts/vanilla-java.sh BelowZeroRetrogenGolden.java inputs.snbt golden.txt */
public class BelowZeroRetrogenGolden {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        PrintWriter out = new PrintWriter(new FileWriter(args[1]));
        TagParser<Tag> parser = TagParser.create(NbtOps.INSTANCE);
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isEmpty()) continue;
            Tag tag;
            try { tag = parser.parseFully(line); } catch (Exception e) { out.println("IN\t" + line + "\tPARSEERR"); continue; }
            // CompoundTag.read(...) uses resultOrPartial
            Optional<BelowZeroRetrogen> r = BelowZeroRetrogen.CODEC.parse(NbtOps.INSTANCE, tag).resultOrPartial(e -> {});
            if (r.isEmpty()) { out.println("IN\t" + line + "\tNONE"); continue; }
            BelowZeroRetrogen b = r.get();
            StringBuilder holes = new StringBuilder();
            for (int z = 0; z < 16; z++) for (int x = 0; x < 16; x++) holes.append(b.hasBedrockHole(x, z) ? '1' : '0');
            // x/z are masked with & 15
            boolean masked = b.hasBedrockHole(16 + 3, -16 + 5) == b.hasBedrockHole(3, 5);
            Tag enc = BelowZeroRetrogen.CODEC.encodeStart(NbtOps.INSTANCE, b).getOrThrow();
            out.println("IN\t" + line + "\tOK\t" + b.targetStatus().getName() + "\t" + b.hasBedrockHoles() + "\t" + masked + "\t" + holes + "\t" + enc);
        }
        out.close();
    }
}
