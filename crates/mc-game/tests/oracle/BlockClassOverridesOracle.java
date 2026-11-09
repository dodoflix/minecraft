// Lists, for every block class of the registry (and its ancestors up to Block), the BlockBehaviour/Block methods it overrides.
// Usage: scripts/golden-block-behaviour.sh overrides
// Output: crates/mc-game/tests/fixtures/block_class_overrides.txt, one line per class: `Class method,method,...` (sorted).
// Read by the guard test `block_class_binding` in crates/mc-game/tests/golden/block_class_binding.rs.
import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;

public class BlockClassOverridesOracle {
   static boolean declaredBy(Class<?> base, Method m) {
      try {
         base.getDeclaredMethod(m.getName(), m.getParameterTypes());
         return true;
      } catch (NoSuchMethodException e) {
         return false;
      }
   }

   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      TreeMap<String, TreeSet<String>> classes = new TreeMap<>();
      for (Block b : BuiltInRegistries.BLOCK) {
         for (Class<?> c = b.getClass(); c != null && c != Block.class && c != BlockBehaviour.class; c = c.getSuperclass()) {
            if (c.getSimpleName().isEmpty()) continue;
            TreeSet<String> methods = classes.computeIfAbsent(c.getSimpleName(), k -> new TreeSet<>());
            for (Method m : c.getDeclaredMethods()) {
               if (m.isSynthetic() || Modifier.isStatic(m.getModifiers()) || Modifier.isPrivate(m.getModifiers())) continue;
               if (declaredBy(BlockBehaviour.class, m) || declaredBy(Block.class, m)) methods.add(m.getName());
            }
         }
      }
      StringBuilder out = new StringBuilder();
      classes.forEach((k, v) -> out.append(k).append(' ').append(String.join(",", v)).append('\n'));
      System.out.print(out);
   }
}
