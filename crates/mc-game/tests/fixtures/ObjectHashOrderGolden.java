// Golden for fastutil Object2ObjectOpenHashMap iteration orders (String keys), as used by Scoreboard.
// Usage: scripts/vanilla-java.sh -o crates/mc-game-core/tests/fixtures/object_hash_order.txt crates/mc-game/tests/fixtures/ObjectHashOrderGolden.java crates/mc-game-core/tests/fixtures/object_hash_order.txt
// Output: crates/mc-game-core/tests/fixtures/object_hash_order.txt
import it.unimi.dsi.fastutil.objects.Object2ObjectOpenHashMap;
import java.io.PrintWriter;
import java.util.*;
import java.util.stream.*;

public class ObjectHashOrderGolden {
    public static void main(String[] args) throws Exception {
        Random rnd = new Random(12345);
        try (PrintWriter out = new PrintWriter(args[0])) {
            for (int round = 0; round < 60; round++) {
                boolean half = round % 2 == 0;
                Object2ObjectOpenHashMap<String, String> m = half ? new Object2ObjectOpenHashMap<>(16, 0.5F) : new Object2ObjectOpenHashMap<>();
                int pool = 4 + rnd.nextInt(120);
                int ops = 1 + rnd.nextInt(300);
                StringBuilder sb = new StringBuilder();
                for (int i = 0; i < ops; i++) {
                    String k = (rnd.nextBoolean() ? "k" : "Player") + rnd.nextInt(pool) + (rnd.nextInt(5) == 0 ? "\u00e9\u4e2d" : "");
                    if (rnd.nextInt(3) == 0) { m.remove(k); sb.append("-").append(k).append(' '); }
                    else { m.put(k, "v"); sb.append("+").append(k).append(' '); }
                }
                out.println((half ? "H " : "D ") + sb.toString().trim());
                out.println("I " + String.join(" ", m.keySet()));
                List<String> fe = new ArrayList<>();
                m.forEach((k, v) -> fe.add(k));
                out.println("F " + String.join(" ", fe));
                out.println("S " + String.join(" ", m.keySet().stream().toList()));
                out.println("V " + String.join(" ", m.values().stream().map(v -> "v").toList()).length());
            }
        }
    }
}
