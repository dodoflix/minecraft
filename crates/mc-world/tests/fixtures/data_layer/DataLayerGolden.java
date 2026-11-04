import java.io.*;
import java.util.*;
import net.minecraft.world.level.chunk.DataLayer;

/** Usage (repo root): scripts/vanilla-java.sh DataLayerGolden.java golden.txt */
public class DataLayerGolden {
    static String hex(byte[] b) { StringBuilder sb = new StringBuilder(); for (byte x : b) sb.append(String.format("%02x", x & 0xFF)); return sb.toString(); }

    static void state(PrintWriter out, DataLayer d) {
        out.println("STATE " + (d.isDefinitelyHomogenous() ? 1 : 0) + (d.isEmpty() ? 1 : 0) + (d.isDefinitelyFilledWith(0) ? 1 : 0)
            + (d.isDefinitelyFilledWith(15) ? 1 : 0) + (d.isDefinitelyFilledWith(7) ? 1 : 0));
    }

    public static void main(String[] args) throws Exception {
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Random rnd = new Random(42);
        // constructor sizes
        for (int n : new int[] {0, 1, 2047, 2048, 2049, 4096}) {
            try { new DataLayer(new byte[n]); out.println("SIZE " + n + " ok"); }
            catch (IllegalArgumentException e) { out.println("SIZE " + n + " " + e.getMessage()); }
        }
        // filled constructors and the packed byte
        for (int v : new int[] {0, 1, 5, 15, 16, 17, 255, -1, -16, 20}) {
            DataLayer d = new DataLayer(v);
            out.println("FILL " + v + " get=" + d.get(3, 4, 5));
            state(out, d);
            byte[] data = d.getData();
            out.println("FILLDATA " + v + " " + (data[0] & 0xFF) + " " + (data[2047] & 0xFF) + " " + new HashSet<>(Arrays.asList(boxed(data))).size());
            state(out, d);
        }
        String[] kinds = {"empty", "filled15", "filled3", "bytes"};
        for (String kind : kinds) {
            DataLayer d;
            out.println("SCN " + kind);
            switch (kind) {
                case "empty" -> d = new DataLayer();
                case "filled15" -> d = new DataLayer(15);
                case "filled3" -> d = new DataLayer(3);
                default -> { byte[] b = new byte[2048]; rnd.nextBytes(b); out.println("INIT " + hex(b)); d = new DataLayer(b); }
            }
            for (int op = 0; op < 600; op++) {
                int x = rnd.nextInt(16), y = rnd.nextInt(16), z = rnd.nextInt(16);
                int r = rnd.nextInt(20);
                if (r < 9) {
                    int v = rnd.nextInt(48) - 16;
                    d.set(x, y, z, v);
                    out.println("S " + x + " " + y + " " + z + " " + v);
                } else if (r < 17) {
                    out.println("G " + x + " " + y + " " + z + " " + d.get(x, y, z));
                } else if (r == 17) {
                    int v = rnd.nextInt(18);
                    d.fill(v);
                    out.println("F " + v);
                } else if (r == 18) {
                    DataLayer c = d.copy();
                    d.set(x, y, z, 7);
                    out.println("C " + x + " " + y + " " + z + " " + c.get(x, y, z) + " " + d.get(x, y, z));
                    out.println("COPYDATA " + (c.isDefinitelyHomogenous() ? 1 : 0) + " " + (c.isEmpty() ? 1 : 0));
                    d = c;
                } else {
                    state(out, d);
                }
            }
            state(out, d);
            out.println("LAYER " + d.layerToString(3).replace('\n', '|'));
            out.println("STR " + d.toString().replace('\n', '|'));
            out.println("DATA " + hex(d.getData()));
            state(out, d);
        }
        out.close();
    }

    static Byte[] boxed(byte[] b) { Byte[] r = new Byte[b.length]; for (int i = 0; i < b.length; i++) r[i] = b[i]; return r; }
}
