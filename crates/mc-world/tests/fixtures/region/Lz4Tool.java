// Golden LZ4Block framing from lz4-java (the library vanilla's RegionFileVersion uses).
//   gen   <dir>: writes <name>.bin (input) and <name>.lz4 (LZ4BlockOutputStream output, default 64 KiB blocks)
//   check <dir>: decodes every <name>.rs.lz4 with LZ4BlockInputStream and compares with <name>.bin
// Run: scripts/vanilla-java.sh crates/mc-world/tests/fixtures/region/Lz4Tool.java gen <dir>
import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.jpountz.lz4.*;

public class Lz4Tool {
    static byte[] repeat(int n) {
        byte[] b = new byte[n];
        byte[] pat = "the quick brown fox jumps over the lazy dog; ".getBytes();
        for (int i = 0; i < n; i++) b[i] = pat[i % pat.length];
        return b;
    }
    static byte[] random(int n, long seed) {
        byte[] b = new byte[n];
        new Random(seed).nextBytes(b);
        return b;
    }
    static byte[] mixed(int n) {
        byte[] b = new byte[n];
        Random r = new Random(7);
        int i = 0;
        while (i < n) {
            int run = 1 + r.nextInt(400);
            boolean rnd = r.nextBoolean();
            byte v = (byte) r.nextInt();
            for (int k = 0; k < run && i < n; k++, i++) b[i] = rnd ? (byte) r.nextInt() : (byte) (v + (k % 3));
        }
        return b;
    }

    public static void main(String[] args) throws Exception {
        Path dir = Paths.get(args[1]);
        if (args[0].equals("gen")) {
            Files.createDirectories(dir);
            Map<String, byte[]> inputs = new LinkedHashMap<>();
            inputs.put("empty", new byte[0]);
            inputs.put("one", new byte[] {42});
            inputs.put("short", "hello lz4 blk".getBytes());
            inputs.put("repeat", repeat(131072 + 10));
            inputs.put("random", random(66000, 1));
            inputs.put("mixed", mixed(70000));
            for (var e : inputs.entrySet()) {
                Files.write(dir.resolve(e.getKey() + ".bin"), e.getValue());
                ByteArrayOutputStream bo = new ByteArrayOutputStream();
                try (OutputStream o = new BufferedOutputStream(new LZ4BlockOutputStream(bo))) {
                    o.write(e.getValue());
                }
                Files.write(dir.resolve(e.getKey() + ".lz4"), bo.toByteArray());
            }
        } else {
            int bad = 0;
            try (var s = Files.list(dir)) {
                for (Path p : (Iterable<Path>) s.sorted()::iterator) {
                    String n = p.getFileName().toString();
                    if (!n.endsWith(".rs.lz4")) continue;
                    byte[] expect = Files.readAllBytes(dir.resolve(n.replace(".rs.lz4", ".bin")));
                    byte[] got;
                    try (InputStream in = new BufferedInputStream(new LZ4BlockInputStream(Files.newInputStream(p)))) {
                        got = in.readAllBytes();
                    } catch (IOException ex) {
                        System.out.println("FAIL " + n + " " + ex);
                        bad++;
                        continue;
                    }
                    boolean ok = Arrays.equals(expect, got);
                    System.out.println((ok ? "OK " : "FAIL ") + n);
                    if (!ok) bad++;
                }
            }
            System.exit(bad == 0 ? 0 : 1);
        }
    }
}
