// Golden generator for `ConstHashReferenceSet` (server/level/fastutil.rs): the iteration order of a fastutil
// `ReferenceOpenHashSet` (vanilla `DistanceManager.chunksToUpdateFutures`) when every identity hash is equal.
// Usage: JAVA_TOOL_OPTIONS="-XX:+UnlockExperimentalVMOptions -XX:hashCode=2" scripts/vanilla-java.sh -o crates/mc-game-core/tests/fixtures/reference_set_order.txt crates/mc-game/tests/fixtures/ReferenceSetOrderGen.java
// Output: crates/mc-game-core/tests/fixtures/reference_set_order.txt
// Line: `<ids added in wave 1> / <ids added in wave 2> ... | <wave 1 order> / <wave 2 order> ...` (the set is cleared after each wave).
import it.unimi.dsi.fastutil.objects.ReferenceOpenHashSet;
import java.util.*;

public class ReferenceSetOrderGen {
    public static void main(String[] args) {
        Object[] objs = new Object[400];
        for (int i = 0; i < objs.length; i++) objs[i] = new Object();
        Map<Object, Integer> ids = new IdentityHashMap<>();
        for (int i = 0; i < objs.length; i++) ids.put(objs[i], i);
        Random random = new Random(1234);
        for (int scenario = 0; scenario < 60; scenario++) {
            ReferenceOpenHashSet<Object> set = new ReferenceOpenHashSet<>();
            StringBuilder in = new StringBuilder(), out = new StringBuilder();
            int waves = 1 + random.nextInt(4);
            for (int w = 0; w < waves; w++) {
                int adds = 1 + random.nextInt(scenario < 40 ? 30 : 300);
                int range = 1 + random.nextInt(400);
                if (w > 0) { in.append(" / "); out.append(" / "); }
                for (int a = 0; a < adds; a++) {
                    int id = random.nextInt(range);
                    in.append(a == 0 ? "" : " ").append(id);
                    set.add(objs[id]);
                }
                boolean first = true;
                for (Object o : set) { out.append(first ? "" : " ").append(ids.get(o)); first = false; }
                set.clear();
            }
            System.out.println(in + " | " + out);
        }
    }
}
