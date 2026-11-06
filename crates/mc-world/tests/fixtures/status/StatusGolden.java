import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.chunk.status.*;

/**
 * Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/status/golden.txt crates/mc-world/tests/fixtures/status/StatusGolden.java crates/mc-world/tests/fixtures/status/inputs.snbt crates/mc-world/tests/fixtures/status/golden.txt
 * Output: crates/mc-world/tests/fixtures/status/golden.txt
 */
public class StatusGolden {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        PrintWriter out = new PrintWriter(new FileWriter(args[1]));
        List<ChunkStatus> all = ChunkStatus.getStatusList();
        out.println("LIST " + String.join(",", all.stream().map(ChunkStatus::getName).toList()));
        for (ChunkStatus s : all) {
            out.println("STATUS " + s.getName() + " " + s.getIndex() + " " + s.getParent().getName() + " " + s.getChunkType()
                + " " + net.minecraft.core.registries.BuiltInRegistries.CHUNK_STATUS.getId(s) + " "
                + String.join(",", s.heightmapsAfter().stream().map(t -> t.getSerializationKey()).toList()));
        }
        for (ChunkStatus a : all) {
            StringBuilder sb = new StringBuilder("CMP " + a.getName());
            for (ChunkStatus b : all) {
                sb.append(' ').append(a.isOrAfter(b) ? 1 : 0).append(a.isAfter(b) ? 1 : 0).append(a.isOrBefore(b) ? 1 : 0).append(a.isBefore(b) ? 1 : 0)
                    .append(ChunkStatus.max(a, b).getIndex());
            }
            out.println(sb);
        }
        out.println("FINAL " + String.join(",", ChunkStatus.FINAL_HEIGHTMAPS.stream().map(t -> t.getSerializationKey()).toList()));
        out.println("MAXDIST " + ChunkStatus.MAX_STRUCTURE_DISTANCE);
        TagParser<Tag> parser = TagParser.create(NbtOps.INSTANCE);
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isEmpty()) continue;
            Tag tag = parser.parseFully(line);
            String byName = tag instanceof StringTag st ? ChunkStatus.byName(st.value()).getName() : "n/a";
            Optional<ChunkStatus> dec = ChunkStatus.CODEC.parse(NbtOps.INSTANCE, tag).result();
            out.println("IN\t" + line + "\t" + byName + "\t" + dec.map(ChunkStatus::getName).orElse("ERR"));
        }
        out.close();
    }
}
