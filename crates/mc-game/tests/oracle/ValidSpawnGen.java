import java.lang.reflect.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;

/** Table of the per-block isValidSpawn overrides (block path + one 0/1 flag per polar bear, ocelot, parrot, zombie, zombified piglin), used by
 *  crates/mc-game-core/src/world/level/block/valid_spawn.rs.
 *  Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/valid_spawn_overrides.txt crates/mc-game/tests/oracle/ValidSpawnGen.java
 *  Output: crates/mc-game/tests/fixtures/valid_spawn_overrides.txt (stdout) */
public class ValidSpawnGen {
   public static void main(String[] a) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      Field f = BlockBehaviour.class.getDeclaredField("properties");
      f.setAccessible(true);
      Field g = BlockBehaviour.Properties.class.getDeclaredField("isValidSpawn");
      g.setAccessible(true);
      Object def = g.get(BlockBehaviour.Properties.of());
      EntityType<?>[] types = {EntityTypes.POLAR_BEAR, EntityTypes.OCELOT, EntityTypes.PARROT, EntityTypes.ZOMBIE, EntityTypes.ZOMBIFIED_PIGLIN};
      for (Block b : BuiltInRegistries.BLOCK) {
         BlockBehaviour.Properties p = (BlockBehaviour.Properties) f.get(b);
         Object v = g.get(p);
         if (v.getClass() == def.getClass()) continue;
         BlockState s = b.defaultBlockState();
         StringBuilder sb = new StringBuilder();
         for (EntityType<?> t : types) sb.append(s.isValidSpawn(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, t) ? '1' : '0');
         System.out.println(BuiltInRegistries.BLOCK.getKey(b).getPath() + " " + sb);
      }
   }
}
