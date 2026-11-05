import java.io.PrintStream;
import java.nio.file.Files;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.ai.navigation.PathNavigation;
import net.minecraft.world.level.pathfinder.Node;
import net.minecraft.world.level.pathfinder.NodeEvaluator;
import net.minecraft.world.level.pathfinder.Path;
import net.minecraft.world.level.pathfinder.PathType;
import net.minecraft.world.level.pathfinder.PathfindingContext;
import net.minecraft.world.level.pathfinder.WalkNodeEvaluator;

/**
 * Pathfinder oracle: replays a scenario (tests/oracle/pathfinder/NAME.txt, also read by tests/golden/pathfinder_golden.rs) against real
 * vanilla and prints the mob profiles, the paths of real {@code mob.getNavigation().createPath(..)} calls (every node with
 * g/h/f/type/walkedDistance/costMalus/closed), and the PathType of positions.
 * <pre>  scripts/entity-oracle.sh crates/mc-game/tests/oracle/PathfinderOracle.java crates/mc-game/tests/oracle/pathfinder/NAME.txt > crates/mc-game/tests/fixtures/pathfinder/NAME.txt</pre>
 * Scenario lines (tokens split on whitespace; # comments):
 * <pre>
 * fill X1 Y1 Z1 X2 Y2 Z2 STATE      block X Y Z STATE        (setBlock flags 3, void world)
 * mob NAME TYPE X Y Z [air] [seed=N] [malus=TYPE:V,..] [doors] [nopass] [float] [fences] [sun]
 *                                   create a vanilla mob (not added to the level), snap to X Y Z, onGround unless `air`, one baseTick
 *                                   (fluid state), random seeded N (default 1), then the options: setPathfindingMalus, nav.setCanOpenDoors,
 *                                   evaluator.setCanPassDoors(false), nav.setCanFloat, setCanWalkOverFences. Prints the MOB profile line.
 * path NAME X Y Z REACH             nav.createPath(x, y, z, reach)           (prints PATH + N lines; the mob keeps its state)
 * pathto NAME X Y Z REACH SPEED     nav.moveTo(...) then PATH of nav.getPath()
 * pathtype NAME X Y Z               WalkNodeEvaluator.getPathTypeStatic(mob, pos) + level.getPathTypeCache().getOrCompute
 * </pre>
 */
public final class PathfinderOracle {
   static String f(float v) { return Float.toString(v); }
   static String f(double v) { return Double.toString(v); }

   public static void main(String[] args) throws Exception {
      try {
         run(args);
      } catch (Throwable t) {
         VanillaLevel.ERR.println("FATAL " + t);
         StackTraceElement[] st = t.getStackTrace();
         for (int i = 0; i < Math.min(st.length, 25); i++) VanillaLevel.ERR.println("  at " + st[i]);
         if (t.getCause() != null) VanillaLevel.ERR.println("cause " + t.getCause());
         Runtime.getRuntime().halt(1);
      }
   }

   static void run(String[] args) throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "void";
      VanillaLevel v;
      try {
         v = VanillaLevel.boot(o);
      } catch (Throwable t) {
         t.printStackTrace(VanillaLevel.ERR);
         Runtime.getRuntime().halt(1);
         return;
      }
      PrintStream out = VanillaLevel.OUT;
      Map<String, Mob> mobs = new LinkedHashMap<>();
      List<String> lines = Files.readAllLines(java.nio.file.Path.of(args[0]));
      try {
         for (String raw : lines) {
            String line = raw.strip();
            if (line.isEmpty() || line.startsWith("#")) {
               if (line.startsWith("#!")) out.println(line);
               continue;
            }
            String[] a = line.split("\\s+");
            switch (a[0]) {
               case "fill" -> {
                  var st = BlockStateParser.parseForBlock(v.server.registryAccess().lookupOrThrow(Registries.BLOCK), a[7], false).blockState();
                  int x1 = Integer.parseInt(a[1]), y1 = Integer.parseInt(a[2]), z1 = Integer.parseInt(a[3]);
                  int x2 = Integer.parseInt(a[4]), y2 = Integer.parseInt(a[5]), z2 = Integer.parseInt(a[6]);
                  for (int x = Math.min(x1, x2); x <= Math.max(x1, x2); x++)
                     for (int y = Math.min(y1, y2); y <= Math.max(y1, y2); y++)
                        for (int z = Math.min(z1, z2); z <= Math.max(z1, z2); z++)
                           v.level.setBlock(new BlockPos(x, y, z), st, 3);
               }
               case "block" -> {
                  var st = BlockStateParser.parseForBlock(v.server.registryAccess().lookupOrThrow(Registries.BLOCK), a[4], false).blockState();
                  v.level.setBlock(new BlockPos(Integer.parseInt(a[1]), Integer.parseInt(a[2]), Integer.parseInt(a[3])), st, 3);
               }
               case "mob" -> {
                  EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.getValue(Identifier.parse("minecraft:" + a[2]));
                  Mob mob = (Mob)type.create(v.level, EntitySpawnReason.COMMAND);
                  mob.snapTo(Double.parseDouble(a[3]), Double.parseDouble(a[4]), Double.parseDouble(a[5]), 0f, 0f);
                  boolean air = Arrays.asList(a).contains("air");
                  mob.setOnGround(!air);
                  long seed = 1;
                  for (int i = 6; i < a.length; i++) if (a[i].startsWith("seed=")) seed = Long.parseLong(a[i].substring(5));
                  mob.baseTick();
                  mob.getRandom().setSeed(seed);
                  PathNavigation nav = mob.getNavigation();
                  NodeEvaluator ev = nav.getNodeEvaluator();
                  for (int i = 6; i < a.length; i++) {
                     String t = a[i];
                     if (t.startsWith("malus=")) {
                        for (String m : t.substring(6).split(",")) {
                           String[] kv = m.split(":");
                           mob.setPathfindingMalus(PathType.valueOf(kv[0]), Float.parseFloat(kv[1]));
                        }
                     } else if (t.equals("doors")) nav.setCanOpenDoors(true);
                     else if (t.equals("nopass")) ev.setCanPassDoors(false);
                     else if (t.equals("float")) nav.setCanFloat(true);
                     else if (t.equals("fences")) ev.setCanWalkOverFences(true);
                     else if (t.equals("sun")) ((net.minecraft.world.entity.ai.navigation.GroundPathNavigation)nav).setAvoidSun(true);
                  }
                  mobs.put(a[1], mob);
                  StringBuilder m = new StringBuilder();
                  for (PathType p : PathType.values()) m.append(p.ordinal() == 0 ? "" : ",").append(f(mob.getPathfindingMalus(p)));
                  var bb = mob.getBoundingBox();
                  out.println("MOB " + a[1] + " type=" + a[2] + " nav=" + navClass(nav) + " pos=" + f(mob.getX()) + "," + f(mob.getY()) + "," + f(mob.getZ())
                     + " bb=" + f(bb.minX) + "," + f(bb.minY) + "," + f(bb.minZ) + "," + f(bb.maxX) + "," + f(bb.maxY) + "," + f(bb.maxZ)
                     + " w=" + f(mob.getBbWidth()) + " h=" + f(mob.getBbHeight()) + " ground=" + mob.onGround() + " water=" + mob.isInWater() + " liquid=" + mob.isInLiquid()
                     + " floatable=" + mob.isInFloatableFluid() + " step=" + f(mob.maxUpStep()) + " fall=" + mob.getMaxFallDistance()
                     + " follow=" + f(mob.getAttributeBaseValue(Attributes.FOLLOW_RANGE)) + " followv=" + f(mob.getAttributeValue(Attributes.FOLLOW_RANGE))
                     + " eval=" + ev.canPassDoors() + "," + ev.canOpenDoors() + "," + ev.canFloat() + "," + ev.canWalkOverFences()
                     + " reqlen=" + reqLen(nav) + " powder=" + net.minecraft.world.level.block.PowderSnowBlock.canEntityWalkOnPowderSnow(mob) + " passenger=" + mob.isPassenger() + " seed=" + seed + " malus=" + m);
               }
               case "path", "pathto" -> {
                  Mob mob = mobs.get(a[1]);
                  PathNavigation nav = mob.getNavigation();
                  Path p;
                  boolean move = a[0].equals("pathto");
                  double x = Double.parseDouble(a[2]), y = Double.parseDouble(a[3]), z = Double.parseDouble(a[4]);
                  if (move) {
                     boolean r = nav.moveTo(x, y, z, Integer.parseInt(a[5]), Double.parseDouble(a[6]));
                     out.println("MOVETO " + a[1] + " " + r);
                     p = nav.getPath();
                  } else {
                     p = nav.createPath(x, y, z, Integer.parseInt(a[5]));
                  }
                  dumpPath(out, a[1], p, nav);
               }
               case "pathset" -> {
                  // pathset NAME REACH X Y Z [X Y Z ...]: nav.createPath(Set<BlockPos>, reach) with the set in this (insertion) order
                  Mob mob = mobs.get(a[1]);
                  Set<BlockPos> set = new LinkedHashSet<>();
                  for (int i = 3; i + 2 < a.length; i += 3) set.add(new BlockPos(Integer.parseInt(a[i]), Integer.parseInt(a[i + 1]), Integer.parseInt(a[i + 2])));
                  dumpPath(out, a[1], mob.getNavigation().createPath(set, Integer.parseInt(a[2])), mob.getNavigation());
               }
               case "malus" -> {
                  Mob mob = mobs.get(a[1]);
                  StringBuilder m = new StringBuilder();
                  for (PathType p : PathType.values()) m.append(p.ordinal() == 0 ? "" : ",").append(f(mob.getPathfindingMalus(p)));
                  out.println("MALUS " + a[1] + " " + m);
               }
               case "pathtype" -> {
                  Mob mob = mobs.get(a[1]);
                  BlockPos pos = new BlockPos(Integer.parseInt(a[2]), Integer.parseInt(a[3]), Integer.parseInt(a[4]));
                  PathType s = WalkNodeEvaluator.getPathTypeStatic(mob, pos);
                  PathType c = v.level.getPathTypeCache().getOrCompute(v.level, pos);
                  PathType c2 = v.level.getPathTypeCache().getOrCompute(v.level, pos);
                  PathType nodeEv = mob.getNavigation().getNodeEvaluator().getPathType(mob, pos);
                  out.println("PT " + a[1] + " " + pos.getX() + "," + pos.getY() + "," + pos.getZ() + " static=" + s + " cache=" + c + " cache2=" + c2 + " evaluator=" + nodeEv);
               }
               default -> throw new IllegalArgumentException(line);
            }
         }
      } catch (Throwable t) {
         VanillaLevel.ERR.println("scenario " + args[0]);
         t.printStackTrace(VanillaLevel.ERR);
         out.flush();
         Runtime.getRuntime().halt(1);
      }
      out.flush();
      System.exit(0);
   }

   /** The standard navigation class a (possibly anonymous/mob-defined) navigation extends. */
   static String navClass(PathNavigation nav) {
      if (nav instanceof net.minecraft.world.entity.ai.navigation.WallClimberNavigation) return "WallClimberNavigation";
      if (nav instanceof net.minecraft.world.entity.ai.navigation.GroundPathNavigation) return "GroundPathNavigation";
      if (nav instanceof net.minecraft.world.entity.ai.navigation.WaterBoundPathNavigation) return "WaterBoundPathNavigation";
      if (nav instanceof net.minecraft.world.entity.ai.navigation.FlyingPathNavigation) return "FlyingPathNavigation";
      if (nav instanceof net.minecraft.world.entity.ai.navigation.AmphibiousPathNavigation) return "AmphibiousPathNavigation";
      throw new IllegalStateException(nav.getClass().getName());
   }

   static String reqLen(PathNavigation nav) {
      try {
         var fld = PathNavigation.class.getDeclaredField("requiredPathLength");
         fld.setAccessible(true);
         return f(fld.getFloat(nav));
      } catch (ReflectiveOperationException e) {
         throw new IllegalStateException(e);
      }
   }

   static void dumpPath(PrintStream out, String name, Path p, PathNavigation nav) {
      if (p == null) {
         out.println("PATH " + name + " null");
         return;
      }
      BlockPos t = p.getTarget();
      out.println("PATH " + name + " reached=" + p.canReach() + " count=" + p.getNodeCount() + " dist=" + f(p.getDistToTarget()) + " target=" + t.getX() + "," + t.getY() + "," + t.getZ()
         + " next=" + p.getNextNodeIndex() + " navtarget=" + (nav.getTargetPos() == null ? "null" : nav.getTargetPos().getX() + "," + nav.getTargetPos().getY() + "," + nav.getTargetPos().getZ()));
      for (int i = 0; i < p.getNodeCount(); i++) {
         Node n = p.getNode(i);
         out.println("N " + i + " " + n.x + " " + n.y + " " + n.z + " g=" + f(n.g) + " h=" + f(n.h) + " f=" + f(n.f) + " type=" + n.type + " walked=" + f(n.walkedDistance)
            + " malus=" + f(n.costMalus) + " closed=" + n.closed);
      }
   }
}
