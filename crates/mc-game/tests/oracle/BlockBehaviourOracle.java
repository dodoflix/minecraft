// Golden generator for the block behaviour classes: for every block state it evaluates canSurvive / updateShape (6
// directions) / rotate (4) / mirror (3) / getPostProcessPos / getVisualShape / getShape / getCollisionShape / getBlockSupportShape / getInteractionShape in K pseudo-random worlds over a stub
// LevelReader and prints one 64-bit hash per block (or the detail per state with `verbose <block>`).
// Usage: scripts/golden-block-behaviour.sh   (`verbose <block>` prints the per-state detail instead)
// Output: crates/mc-game/tests/fixtures/block_behaviour.txt
// The world function, hashing and pools are mirrored 1:1 by crates/mc-game/tests/golden/block_behaviour_golden.rs.
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.material.*;
import net.minecraft.world.ticks.*;

public class BlockBehaviourOracle {
   static final int WORLDS = 16;
   static final int POOL_CAP = 24;
   static final int CLASS_CAP = 200;
   static final BlockPos ORIGIN = new BlockPos(100, 70, -37);

   static List<BlockState> global = new ArrayList<>();
   static List<BlockState> own = List.of();
   static List<BlockState> mates = List.of();
   static long worldSeed;
   static BlockState center;
   static final List<long[]> ticks = new ArrayList<>();

   static long mix(long z) {
      z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
      z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
      return z ^ (z >>> 31);
   }

   static long cellHash(int x, int y, int z) {
      return mix(worldSeed * 0x9E3779B97F4A7C15L + ((long)x * 73856093L ^ (long)y * 19349663L ^ (long)z * 83492791L));
   }

   static BlockState cell(int x, int y, int z) {
      if (x == ORIGIN.getX() && y == ORIGIN.getY() && z == ORIGIN.getZ()) return center;
      long h = cellHash(x, y, z);
      int r = (int)Long.remainderUnsigned(h, 100);
      long h2 = mix(h);
      List<BlockState> pool;
      if (r < 25) return Blocks.AIR.defaultBlockState();
      else if (r < 55) pool = global;
      else if (r < 80) pool = own;
      else pool = mates;
      return pool.get((int)Long.remainderUnsigned(h2, pool.size()));
   }

   static List<BlockState> capped(Block b, int cap) {
      List<BlockState> all = b.getStateDefinition().getPossibleStates();
      return all.size() <= cap ? all : all.subList(0, cap);
   }

   static String name(Block b) { return BuiltInRegistries.BLOCK.getKey(b).getPath(); }

   static Object proxy;
   static net.minecraft.core.RegistryAccess registries;

   static Object dflt(Object p, Method m, Object[] a) throws Throwable {
      if (m.isDefault()) return InvocationHandler.invokeDefault(p, m, a);
      throw new UnsupportedOperationException("stub: " + m);
   }

   static class Recorder implements InvocationHandler {
      final boolean fluid;
      Recorder(boolean fluid) { this.fluid = fluid; }
      public Object invoke(Object p, Method m, Object[] a) throws Throwable {
         switch (m.getName()) {
            case "schedule": {
               ScheduledTick<?> t = (ScheduledTick<?>)a[0];
               BlockPos q = t.pos();
               int id = fluid ? BuiltInRegistries.FLUID.getId((Fluid)t.type()) : BuiltInRegistries.BLOCK.getId((Block)t.type());
               ticks.add(new long[]{fluid ? 1 : 0, id, q.getX() - ORIGIN.getX(), q.getY() - ORIGIN.getY(), q.getZ() - ORIGIN.getZ(), t.triggerTick(), t.priority().ordinal()});
               return null;
            }
            case "hasScheduledTick": return false;
            case "willTickThisTick": return false;
            case "count": return 0;
            default: return dflt(p, m, a);
         }
      }
   }

   static long brightness(int x, int y, int z, int layer) { return Math.floorMod((long)x * 3 + (long)y * 5 + (long)z * 7 + layer * 11L, 16); }

   static class Level implements InvocationHandler {
      final Object blockTicks = Proxy.newProxyInstance(Level.class.getClassLoader(), new Class[]{LevelTickAccess.class}, new Recorder(false));
      final Object fluidTicks = Proxy.newProxyInstance(Level.class.getClassLoader(), new Class[]{LevelTickAccess.class}, new Recorder(true));
      public Object invoke(Object p, Method m, Object[] a) throws Throwable {
         switch (m.getName()) {
            case "getBlockState": { BlockPos q = (BlockPos)a[0]; return cell(q.getX(), q.getY(), q.getZ()); }
            case "getFluidState": { BlockPos q = (BlockPos)a[0]; return cell(q.getX(), q.getY(), q.getZ()).getFluidState(); }
            case "getBlockEntity": return null;
            case "getMinY": return -64;
            case "getHeight": if (a == null) return 384; if (a.length == 2) return dflt(p, m, a); { int x = (Integer)a[1], z = (Integer)a[2]; return 60 + Math.floorMod(x * 3 + z * 5, 8); }
            case "isClientSide": return false;
            case "getSeaLevel": return 63;
            case "hasChunk": return true;
            case "getSkyDarken": return 0;
            case "enabledFeatures": return net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS;
            case "getBrightness": { BlockPos q = (BlockPos)a[1]; return (int)brightness(q.getX(), q.getY(), q.getZ(), ((LightLayer)a[0]).ordinal()); }
            case "getRawBrightness": { BlockPos q = (BlockPos)a[0]; return (int)brightness(q.getX(), q.getY(), q.getZ(), 5); }
            case "canSeeSky": { BlockPos q = (BlockPos)a[0]; return brightness(q.getX(), q.getY(), q.getZ(), 9) < 8; }
            case "environmentAttributes": return net.minecraft.world.attribute.EnvironmentAttributeReader.EMPTY;
            case "registryAccess": return registries;
            case "getBlockTicks": return blockTicks;
            case "getFluidTicks": return fluidTicks;
            case "createTick": {
               BlockPos q = (BlockPos)a[0];
               TickPriority pr = a.length == 4 ? (TickPriority)a[3] : TickPriority.NORMAL;
               return new ScheduledTick<>(a[1], q, (long)(Integer)a[2], pr, 0L);
            }
            default: return dflt(p, m, a);
         }
      }
   }

   static StringBuilder verbose;

   static long hashShape(long h, net.minecraft.world.phys.shapes.VoxelShape shape) {
      java.util.List<net.minecraft.world.phys.AABB> boxes = shape.toAabbs();
      h = mix(h ^ boxes.size());
      for (net.minecraft.world.phys.AABB box : boxes) {
         for (double c : new double[]{box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ}) h = mix(h ^ Double.doubleToLongBits(c));
      }
      return h;
   }

   static long evalState(BlockState s) throws Exception {
      long h = 0x1234567L;
      int sid = Block.getId(s);
      for (Rotation_ r : Rotation_.values()) h = mix(h ^ Block.getId(s.rotate(r.r)));
      for (Mirror_ m : Mirror_.values()) h = mix(h ^ Block.getId(s.mirror(m.m)));
      center = s;
      for (int w = 0; w < WORLDS; w++) {
         worldSeed = w + 1;
         ticks.clear();
         LevelReader level = (LevelReader)proxy;
         ScheduledTickAccess tick = (ScheduledTickAccess)proxy;
         boolean surv = s.canSurvive(level, ORIGIN);
         h = mix(h ^ (surv ? 1 : 2));
         BlockPos pp = s.getPostProcessPos(level, ORIGIN);
         h = mix(h ^ (pp == null ? 7 : ((long)(pp.getX() - ORIGIN.getX()) * 31 + (pp.getY() - ORIGIN.getY()) * 5 + (pp.getZ() - ORIGIN.getZ()))));
         h = hashShape(h, s.getVisualShape(level, ORIGIN, net.minecraft.world.phys.shapes.CollisionContext.empty()));
         h = hashShape(h, s.getShape(level, ORIGIN));
         h = hashShape(h, s.getCollisionShape(level, ORIGIN));
         h = hashShape(h, s.getBlockSupportShape(level, ORIGIN));
         h = hashShape(h, s.getInteractionShape(level, ORIGIN));
         if (verbose != null) verbose.append("S").append(sid).append(" w").append(w).append(" surv=").append(surv).append(" pp=").append(pp == null ? "null" : pp.toString());
         for (int di = 0; di < 6; di++) {
            Direction d = UPDATE_ORDER[di];
            BlockPos np = ORIGIN.relative(d);
            BlockState ns = level.getBlockState(np);
            ticks.clear();
            RandomSource rnd = new net.minecraft.world.level.levelgen.LegacyRandomSource(sid * 31L + w * 7L + di);
            BlockState out = s.updateShape(level, tick, ORIGIN, d, np, ns, rnd);
            h = mix(h ^ Block.getId(out));
            h = mix(h ^ rnd.nextLong());
            for (long[] t : ticks) for (long v : t) h = mix(h ^ v);
            h = mix(h ^ ticks.size());
            if (verbose != null) {
               verbose.append(" |").append(d).append("->").append(Block.getId(out)).append(" rnd=").append(Long.toHexString(rnd.nextLong() == 0 ? 0 : 1));
               for (long[] t : ticks) verbose.append(" t").append(Arrays.toString(t));
            }
         }
         if (verbose != null) verbose.append('\n');
      }
      return h;
   }

   enum Rotation_ { A(net.minecraft.world.level.block.Rotation.NONE), B(net.minecraft.world.level.block.Rotation.CLOCKWISE_90), C(net.minecraft.world.level.block.Rotation.CLOCKWISE_180), D(net.minecraft.world.level.block.Rotation.COUNTERCLOCKWISE_90);
      final net.minecraft.world.level.block.Rotation r; Rotation_(net.minecraft.world.level.block.Rotation r) { this.r = r; } }
   enum Mirror_ { A(net.minecraft.world.level.block.Mirror.NONE), B(net.minecraft.world.level.block.Mirror.LEFT_RIGHT), C(net.minecraft.world.level.block.Mirror.FRONT_BACK);
      final net.minecraft.world.level.block.Mirror m; Mirror_(net.minecraft.world.level.block.Mirror m) { this.m = m; } }

   static final Direction[] UPDATE_ORDER = {Direction.WEST, Direction.EAST, Direction.NORTH, Direction.SOUTH, Direction.DOWN, Direction.UP};

   public static void main(String[] args) throws Exception {
      registries = extractor.Setup.bootstrap();
      proxy = Proxy.newProxyInstance(BlockBehaviourOracle.class.getClassLoader(), new Class[]{LevelReader.class, ScheduledTickAccess.class}, new Level());
      for (String n : Files.readAllLines(Path.of(args[0]))) {
         if (n.isBlank()) continue;
         Block b = BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(n));
         global.addAll(capped(b, POOL_CAP));
      }
      String only = args.length > 2 && args[1].equals("verbose") ? args[2] : null;
      StringBuilder out = new StringBuilder();
      for (Block b : BuiltInRegistries.BLOCK) {
         String nm = name(b);
         if (only != null && !only.equals(nm)) continue;
         own = b.getStateDefinition().getPossibleStates();
         List<BlockState> m = new ArrayList<>();
         for (Block o : BuiltInRegistries.BLOCK) {
            if (o.getClass() == b.getClass() && m.size() < CLASS_CAP) m.addAll(capped(o, CLASS_CAP - m.size()));
         }
         mates = m;
         long h = 0;
         verbose = only != null ? new StringBuilder() : null;
         for (BlockState s : own) h = mix(h ^ evalState(s));
         out.append(nm).append(' ').append(Long.toHexString(h)).append('\n');
         if (verbose != null) out.append(verbose);
      }
      System.out.print(out);
   }
}
