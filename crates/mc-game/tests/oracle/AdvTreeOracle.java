import java.nio.file.*;
import java.util.*;
import net.minecraft.advancements.AdvancementHolder;
import net.minecraft.advancements.AdvancementNode;
import net.minecraft.advancements.AdvancementTree;
import net.minecraft.server.advancements.AdvancementVisibilityEvaluator;

/**
 * Vanilla oracle of the advancement tree (`advancements::advancement_tree`, `tree_node_position` and
 * `advancement_visibility_evaluator` in mc-game-core): the layout of every node of the default data pack and what the visibility evaluator
 * reports for a few progress states.
 *
 * Usage: scripts/adv-tree-oracle.sh   (runs crates/mc-game/tests/oracle/scenarios/adv_tree/NAME.txt through the in-process vanilla level)
 * Output: crates/mc-game/tests/fixtures/adv_tree/NAME.txt
 *
 * <p>A scenario is an {@code EntityOracleDriver} option header (`level flat`, `preload 1`) and `state NAME SPEC` lines. SPEC is `-` (nothing is
 * done) or a comma list of advancement ids, `prefix:ID_PREFIX` (every advancement whose id starts with it) and `*` (every advancement).
 * <pre>
 * NODE id parent x y        every node of the tree in tree order (`parent` = `-` for a root; x and y as Float.toString)
 * STATE name                the next lines are the evaluator output for that state
 * VIS id                   a node AdvancementVisibilityEvaluator.evaluateVisibility over every root reports visible, in the order it calls its output
 * HIDDEN n                 how many nodes it reports invisible (every node is reported once)
 * </pre>
 * The sibling order of a node follows insertion order (the oracle's shadow of AdvancementNode, see shadow.patch): vanilla's identity-hashed
 * set has no reproducible order.
 */
public final class AdvTreeOracle {
   public static void main(String[] args) throws Exception {
      EntityOracleDriver d = new EntityOracleDriver();
      List<String> lines = Files.readAllLines(Path.of(args[0]));
      Map<String, String> states = new LinkedHashMap<>();
      try {
         for (int i = 0; i < lines.size(); i++) {
            d.lineNo = i + 1;
            d.line = lines.get(i).strip();
            if (d.line.isEmpty() || d.line.startsWith("#")) {
               continue;
            }
            List<String> a = EntityOracleDriver.tokens(d.line);
            if (a.get(0).equals("state")) {
               states.put(a.get(1), a.get(2));
            } else {
               d.run(a);
            }
         }
         d.world();
         AdvancementTree tree = d.v.server.getAdvancements().tree();
         for (AdvancementNode node : tree.nodes()) {
            d.out.println("NODE " + node.holder().id() + " " + (node.parent() == null ? "-" : node.parent().holder().id()) + " " + node.x() + " " + node.y());
         }
         for (var state : states.entrySet()) {
            Set<String> exact = new HashSet<>();
            List<String> prefixes = new ArrayList<>();
            boolean all = false;
            for (String item : state.getValue().split(",")) {
               if (item.equals("*")) {
                  all = true;
               } else if (item.startsWith("prefix:")) {
                  prefixes.add(item.substring("prefix:".length()));
               } else if (!item.equals("-")) {
                  exact.add(item);
               }
            }
            final boolean everything = all;
            d.out.println("STATE " + state.getKey());
            int[] hidden = {0};
            for (AdvancementNode root : tree.roots()) {
               AdvancementVisibilityEvaluator.evaluateVisibility(root, node -> {
                  String id = node.holder().id().toString();
                  return everything || exact.contains(id) || prefixes.stream().anyMatch(id::startsWith);
               }, (node, visible) -> {
                  if (visible) {
                     d.out.println("VIS " + node.holder().id());
                  } else {
                     hidden[0]++;
                  }
               });
            }
            d.out.println("HIDDEN " + hidden[0]);
         }
      } catch (Throwable t) {
         VanillaLevel.ERR.println("scenario " + args[0] + ":" + d.lineNo + ": " + d.line);
         t.printStackTrace(VanillaLevel.ERR);
         d.out.flush();
         Runtime.getRuntime().halt(1);
      }
      d.out.flush();
      System.exit(0);
   }
}
