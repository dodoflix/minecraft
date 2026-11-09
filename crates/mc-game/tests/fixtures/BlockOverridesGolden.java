import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;

/** The methods each block's Java class chain overrides (anonymous subclasses included), for the block registration guards.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/block_overrides.txt crates/mc-game/tests/fixtures/BlockOverridesGolden.java
 * Output: crates/mc-game/tests/fixtures/block_overrides.txt, one line per block in registry order: `name method@Class,method@Class,...`
 * (sorted, overloads merged, Class = the declaring class, `Anonymous<Parent>` for anonymous ones; `-` = none). A method counts when a class below `Block` declares it and `Block`,
 * `BlockBehaviour` or an interface of the chain declares the same name and parameter types (class-specific virtuals of
 * intermediate classes are not hooks: the registered class of a chain owns them). */
public class BlockOverridesGolden {
  static boolean declaredAbove(Class<?> c, Method m) {
    Deque<Class<?>> todo = new ArrayDeque<>();
    if (c.getSuperclass() != null) todo.add(c.getSuperclass());
    todo.addAll(Arrays.asList(c.getInterfaces()));
    while (!todo.isEmpty()) {
      Class<?> s = todo.poll();
      if (s == Object.class) continue;
      if (s == Block.class || s == BlockBehaviour.class || s.isInterface()) {
        try {
          s.getDeclaredMethod(m.getName(), m.getParameterTypes());
          return true;
        } catch (NoSuchMethodException e) {
        }
      }
      if (s.getSuperclass() != null) todo.add(s.getSuperclass());
      todo.addAll(Arrays.asList(s.getInterfaces()));
    }
    return false;
  }

  public static void main(String[] args) {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    StringBuilder out = new StringBuilder();
    for (Block block : BuiltInRegistries.BLOCK) {
      TreeSet<String> methods = new TreeSet<>();
      for (Class<?> c = block.getClass(); c != null && c != Block.class && c != BlockBehaviour.class; c = c.getSuperclass()) {
        for (Method m : c.getDeclaredMethods()) {
          if (m.isSynthetic() || m.isBridge() || Modifier.isStatic(m.getModifiers()) || Modifier.isPrivate(m.getModifiers())) continue;
          if (declaredAbove(c, m)) methods.add(m.getName() + "@" + (c.getSimpleName().isEmpty() ? "Anonymous" + c.getSuperclass().getSimpleName() : c.getSimpleName()));
        }
      }
      out.append(BuiltInRegistries.BLOCK.getKey(block)).append(' ').append(methods.isEmpty() ? "-" : String.join(",", methods)).append('\n');
    }
    System.out.print(out);
  }
}
