// Vanilla oracle for `IOWorker.isOldChunkAround`: replays a script of chunk writes and queries against a real
// `SimpleRegionStorage` over a fresh region folder.
//   Script lines (whitespace separated, `#` comments):
//     W X Z VERSION BLEND   store a chunk tag; VERSION is an int, `long` (a LongTag DataVersion of 5023) or `none`; BLEND is `blend`
//                           (a blending_data compound), `blendint` (a blending_data IntTag) or `-`
//     D X Z                 store nothing at the chunk (`IOWorker.STORE_EMPTY`)
//     S                     synchronize(true): the queued writes reach the region files
//     Q X Z RANGE           isOldChunkAround(ChunkPos(X, Z), RANGE)
//   Output: one line per Q, `Q X Z RANGE true|false`.
// Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/region/old_chunk_around.expected crates/mc-world/tests/fixtures/region/OldChunkAroundTool.java crates/mc-world/tests/fixtures/region/old_chunk_around.script
// Output: crates/mc-world/tests/fixtures/region/old_chunk_around.expected (stdout captured)
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.datafix.DataFixTypes;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.RegionStorageInfo;
import net.minecraft.world.level.chunk.storage.SimpleRegionStorage;

public class OldChunkAroundTool {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Path dir = Files.createTempDirectory("old-chunk-around");
        RegionStorageInfo info = new RegionStorageInfo("test", ResourceKey.create(Registries.DIMENSION, Identifier.withDefaultNamespace("overworld")), "chunk");
        StringBuilder out = new StringBuilder();
        try (SimpleRegionStorage storage = new SimpleRegionStorage(info, dir, DataFixers.getDataFixer(), false, DataFixTypes.CHUNK)) {
            for (String line : Files.readAllLines(Paths.get(args[0]))) {
                String[] f = line.trim().split("\\s+");
                if (f[0].isEmpty() || f[0].startsWith("#")) continue;
                switch (f[0]) {
                    case "W" -> {
                        CompoundTag tag = new CompoundTag();
                        switch (f[3]) {
                            case "none" -> {}
                            case "long" -> tag.putLong("DataVersion", 5023L);
                            default -> tag.putInt("DataVersion", Integer.parseInt(f[3]));
                        }
                        switch (f[4]) {
                            case "blend" -> {
                                CompoundTag blending = new CompoundTag();
                                blending.putInt("min_section", -4);
                                blending.putInt("max_section", 20);
                                tag.put("blending_data", blending);
                            }
                            case "blendint" -> tag.putInt("blending_data", 1);
                            default -> {}
                        }
                        storage.write(new ChunkPos(Integer.parseInt(f[1]), Integer.parseInt(f[2])), tag);
                    }
                    case "D" -> storage.write(new ChunkPos(Integer.parseInt(f[1]), Integer.parseInt(f[2])), (CompoundTag) null);
                    case "S" -> storage.synchronize(true).join();
                    case "Q" -> {
                        boolean old = storage.isOldChunkAround(new ChunkPos(Integer.parseInt(f[1]), Integer.parseInt(f[2])), Integer.parseInt(f[3]));
                        out.append("Q ").append(f[1]).append(' ').append(f[2]).append(' ').append(f[3]).append(' ').append(old).append('\n');
                    }
                    default -> throw new IllegalArgumentException(line);
                }
            }
        }
        System.out.print(out);
        try (var walk = Files.walk(dir)) {
            walk.sorted(Comparator.reverseOrder()).forEach(p -> p.toFile().delete());
        }
        System.exit(0);
    }
}
