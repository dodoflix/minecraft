// Vanilla RegionFile oracle.
//   dump <dir>        : for every r.X.Z.mca in <dir> prints one line per chunk slot:
//                       <file> <chunkX> <chunkZ> has=<hasChunk> exists=<doesChunkExist> len=<n|null> sha256=<hex|->
//                       (len/sha256 are of the decompressed payload from getChunkDataInputStream; null = vanilla returned null)
//   gen <dir>         : writes r.<id>.0.mca for each compression version (1 gzip, 2 deflate, 3 none, 4 lz4)
//   genbig <dir>      : writes r.3.1.mca / r.4.1.mca containing an oversized (external .mcc) chunk plus a small one
//   write <srcdir> <dstdir> <versionId> : re-reads every chunk of <srcdir> and writes it into <dstdir> with vanilla RegionFile
// Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/region/java/expected.txt crates/mc-world/tests/fixtures/region/RegionTool.java dump crates/mc-world/tests/fixtures/region/java
//        (gen/genbig/write modes: scripts/vanilla-java.sh crates/mc-world/tests/fixtures/region/RegionTool.java gen <dir>, which writes .mca files into the directory directly)
// Output: crates/mc-world/tests/fixtures/region/java/expected.txt (dump, stdout captured); gen/genbig/write write r.*.mca files into their directory argument
import java.io.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.RegionFile;
import net.minecraft.world.level.chunk.storage.RegionFileVersion;
import net.minecraft.world.level.chunk.storage.RegionStorageInfo;

public class RegionTool {
    static final RegionStorageInfo INFO =
        new RegionStorageInfo("test", ResourceKey.create(Registries.DIMENSION, Identifier.withDefaultNamespace("overworld")), "chunk");

    static String hex(byte[] b) throws Exception {
        StringBuilder sb = new StringBuilder();
        for (byte x : MessageDigest.getInstance("SHA-256").digest(b)) sb.append(String.format(java.util.Locale.ROOT, "%02x", x));
        return sb.toString();
    }

    static byte[] payload(int n, long seed, boolean compressible) {
        byte[] b = new byte[n];
        if (compressible) {
            for (int i = 0; i < n; i++) b[i] = (byte) ((i / 7) % 13);
        } else {
            new Random(seed).nextBytes(b);
        }
        return b;
    }

    static void put(RegionFile f, int x, int z, byte[] data) throws IOException {
        try (DataOutputStream o = f.getChunkDataOutputStream(new ChunkPos(x, z))) {
            o.write(data);
        }
    }

    public static void main(String[] args) throws Exception {
        Path dir = Paths.get(args[1]);
        switch (args[0]) {
            case "dump" -> {
                List<Path> files = new ArrayList<>();
                try (var s = Files.list(dir)) {
                    s.filter(p -> p.getFileName().toString().endsWith(".mca")).sorted().forEach(files::add);
                }
                for (Path p : files) {
                    String[] parts = p.getFileName().toString().split("\\.");
                    int rx = Integer.parseInt(parts[1]), rz = Integer.parseInt(parts[2]);
                    RegionFile f = new RegionFile(INFO, p, dir, false);
                    for (int z = 0; z < 32; z++) {
                        for (int x = 0; x < 32; x++) {
                            ChunkPos pos = new ChunkPos(rx * 32 + x, rz * 32 + z);
                            boolean has = f.hasChunk(pos);
                            boolean exists = f.doesChunkExist(pos);
                            String len = "null", sha = "-";
                            if (has) {
                                try (DataInputStream in = f.getChunkDataInputStream(pos)) {
                                    if (in != null) {
                                        byte[] b = in.readAllBytes();
                                        len = String.valueOf(b.length);
                                        sha = hex(b);
                                    }
                                } catch (IOException e) {
                                    len = "ioerror";
                                }
                            }
                            System.out.println(p.getFileName() + " " + pos.x() + " " + pos.z() + " has=" + has + " exists=" + exists + " len=" + len + " sha256=" + sha);
                        }
                    }
                }
            }
            case "gen" -> {
                Files.createDirectories(dir);
                for (int v : new int[] {1, 2, 3, 4}) {
                    RegionFile f = new RegionFile(INFO, dir.resolve("r." + v + ".0.mca"), dir, RegionFileVersion.fromId(v), false);
                    put(f, v * 32 + 0, 0, payload(10, 1, false));
                    put(f, v * 32 + 1, 0, payload(5000, 2, true));
                    put(f, v * 32 + 31, 31, payload(100000, 3, true));
                    put(f, v * 32 + 5, 7, payload(3000, 4, false));
                    put(f, v * 32 + 1, 0, payload(9000, 5, false)); // rewrite: frees and reallocates sectors
                    put(f, v * 32 + 6, 7, payload(4091, 6, false)); // exact sector boundary for version 3
                    f.clear(new ChunkPos(v * 32, 0));
                    f.close();
                }
            }
            case "genbig" -> {
                Files.createDirectories(dir);
                for (int v : new int[] {3, 4}) {
                    RegionFile f = new RegionFile(INFO, dir.resolve("r." + v + ".1.mca"), dir, RegionFileVersion.fromId(v), false);
                    put(f, v * 32 + 2, 32 + 2, payload(300, 1, false));
                    put(f, v * 32 + 3, 32 + 3, payload(1_100_000, 9, false));
                    put(f, v * 32 + 4, 32 + 4, payload(1_048_000, 10, false)); // just under 256 sectors with header (internal)
                    f.close();
                }
            }
            case "write" -> {
                Path dst = Paths.get(args[2]);
                RegionFileVersion version = RegionFileVersion.fromId(Integer.parseInt(args[3]));
                Files.createDirectories(dst);
                try (var s = Files.list(dir)) {
                    for (Path p : (Iterable<Path>) s.filter(q -> q.getFileName().toString().endsWith(".mca")).sorted()::iterator) {
                        String[] parts = p.getFileName().toString().split("\\.");
                        int rx = Integer.parseInt(parts[1]), rz = Integer.parseInt(parts[2]);
                        RegionFile in = new RegionFile(INFO, p, dir, false);
                        RegionFile out = new RegionFile(INFO, dst.resolve(p.getFileName()), dst, version, false);
                        for (int z = 0; z < 32; z++) for (int x = 0; x < 32; x++) {
                            ChunkPos pos = new ChunkPos(rx * 32 + x, rz * 32 + z);
                            try (DataInputStream i = in.getChunkDataInputStream(pos)) {
                                if (i != null) put(out, pos.x(), pos.z(), i.readAllBytes());
                            }
                        }
                        out.close();
                    }
                }
            }
            default -> throw new IllegalArgumentException(args[0]);
        }
    }
}
