// Generates geometry_dir.txt: golden values for Direction/AxisCycle/CompositeDirection/Rotation/Mirror/Vec2/Vec3/AABB.
// Usage: scripts/vanilla-java.sh -o crates/mc-core/tests/fixtures/geometry_dir/geometry_dir.txt crates/mc-core/tests/fixtures/geometry_dir/GeometryDir.java
// Output: crates/mc-core/tests/fixtures/geometry_dir/geometry_dir.txt
// Line format: `op in-tokens = out-tokens`. doubles: 16 hex digits of Double.doubleToLongBits, floats: 8 hex digits of
// Float.floatToIntBits, ints decimal, booleans 0/1, directions/axes by name, absent values `null`, exceptions `ERR`.
import java.lang.reflect.Field;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.core.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Marker;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec2;
import net.minecraft.world.phys.Vec3;

public class GeometryDir {
   static final StringBuilder OUT = new StringBuilder();

   static void line(String op, String in, String out) {
      OUT.append(op).append(' ').append(in).append(" = ").append(out).append('\n');
   }

   static String d(double v) { return String.format("%016x", Double.doubleToLongBits(v)); }
   static String f(float v) { return String.format("%08x", Float.floatToIntBits(v)); }
   static String b(boolean v) { return v ? "1" : "0"; }
   static String v3(Vec3 v) { return v == null ? "null" : d(v.x) + " " + d(v.y) + " " + d(v.z); }
   static String v2(Vec2 v) { return f(v.x) + " " + f(v.y); }
   static String bb(AABB a) { return d(a.minX) + " " + d(a.minY) + " " + d(a.minZ) + " " + d(a.maxX) + " " + d(a.maxY) + " " + d(a.maxZ); }
   static String dir(Direction x) { return x == null ? "null" : x.getName(); }
   static String ax(Direction.Axis x) { return x.getName(); }
   static String guard(Supplier<String> s) {
      try { return s.get(); } catch (RuntimeException e) { return "ERR"; }
   }

   static Entity entity(float xRot, float yRot) throws Exception {
      Field uf = Class.forName("sun.misc.Unsafe").getDeclaredField("theUnsafe");
      uf.setAccessible(true);
      sun.misc.Unsafe u = (sun.misc.Unsafe) uf.get(null);
      Entity e = (Entity) u.allocateInstance(Marker.class);
      Field xf = Entity.class.getDeclaredField("xRot");
      xf.setAccessible(true);
      xf.setFloat(e, xRot);
      Field yf = Entity.class.getDeclaredField("yRot");
      yf.setAccessible(true);
      yf.setFloat(e, yRot);
      return e;
   }

   static final double[] DP = {0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.0E-7, -1.0E-7, 1.0000001E-7, 0.3, 2.7, -3.1, 17.25, -64.5, 1.0E10, 1.0E-300};
   static final double[] SPECIAL = {Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.MIN_VALUE, Double.MAX_VALUE};

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      Random rnd = new Random(20260304L);

      directionTables(rnd);
      axisCycle();
      composite();
      rotationMirror(rnd);
      vec2(rnd);
      vec3s(rnd);
      aabbs(rnd);
      System.out.print(OUT);
   }

   static float[] angles(Random rnd) {
      List<Float> l = new ArrayList<>();
      for (int a = -720; a <= 720; a += 15) l.add((float) a);
      for (int i = 0; i < 60; i++) l.add((rnd.nextFloat() - 0.5F) * 1500.0F);
      l.add(45.0F); l.add(-45.0F); l.add(135.0F); l.add(89.99999F); l.add(90.00001F);
      float[] r = new float[l.size()];
      for (int i = 0; i < r.length; i++) r[i] = l.get(i);
      return r;
   }

   static void directionTables(Random rnd) throws Exception {
      for (Direction x : Direction.values()) {
         line("dir_table", x.getName(),
            x.ordinal() + " " + x.get3DDataValue() + " " + x.get2DDataValue() + " " + ax(x.getAxis()) + " "
               + x.getAxisDirection().getStep() + " " + dir(x.getOpposite()) + " " + x.getStepX() + " " + x.getStepY() + " " + x.getStepZ() + " "
               + f(x.toYRot()) + " " + guard(() -> f(Direction.getYRot(x))) + " " + guard(() -> dir(x.getClockWise())) + " "
               + guard(() -> dir(x.getCounterClockWise())) + " " + v3(x.getUnitVec3()));
         for (Direction.Axis a : Direction.Axis.values()) {
            line("dir_cw_axis", x.getName() + " " + a.getName(), guard(() -> dir(x.getClockWise(a))) + " " + guard(() -> dir(x.getCounterClockWise(a))));
         }
      }
      for (Direction.Axis a : Direction.Axis.values()) {
         line("axis_table", a.getName(), a.ordinal() + " " + b(a.isVertical()) + " " + b(a.isHorizontal()) + " " + dir(a.getPositive()) + " "
            + dir(a.getNegative()) + " " + a.getPlane().name() + " " + a.choose(1, 2, 3) + " " + d(a.choose(1.5, 2.5, 3.5)) + " " + b(a.choose(true, false, false)));
         for (Direction x : Direction.values()) line("axis_test", a.getName() + " " + x.getName(), b(a.test(x)));
         for (Direction.AxisDirection ad : Direction.AxisDirection.values()) {
            line("from_axis_dir", a.getName() + " " + ad.name(), dir(Direction.fromAxisAndDirection(a, ad)) + " " + dir(Direction.get(ad, a)));
         }
      }
      for (Direction.AxisDirection ad : Direction.AxisDirection.values())
         line("axis_direction", ad.name(), ad.getStep() + " " + ad.opposite().name() + " " + ad.getName().replace(' ', '_'));
      for (Direction.Plane p : Direction.Plane.values()) {
         StringBuilder faces = new StringBuilder();
         for (Direction x : p) faces.append(x.getName()).append(',');
         StringBuilder tests = new StringBuilder();
         for (Direction x : Direction.values()) tests.append(b(p.test(x)));
         line("plane", p.name(), p.length() + " " + faces + " " + tests);
      }
      for (int i0 = -14; i0 <= 14; i0++) {
         final int i = i0;
         line("from3d", "" + i, dir(Direction.from3DDataValue(i)) + " " + guard(() -> dir(Direction.from2DDataValue(i))) + " " + dir(Direction.BY_ID.apply(i)));
      }
      line("from3d", "" + Integer.MIN_VALUE, dir(Direction.from3DDataValue(Integer.MIN_VALUE)) + " " + dir(Direction.from2DDataValue(Integer.MIN_VALUE)) + " " + dir(Direction.BY_ID.apply(Integer.MIN_VALUE)));
      List<Double> yrots = new ArrayList<>();
      for (double a = -400; a <= 400; a += 7.5) yrots.add(a);
      for (int i = 0; i < 100; i++) yrots.add((rnd.nextDouble() - 0.5) * 2000);
      yrots.add(44.99999999999999); yrots.add(45.0); yrots.add(-45.0); yrots.add(135.0);
      for (double y : yrots) line("from_yrot", d(y), dir(Direction.fromYRot(y)));
      List<Integer> ints = new ArrayList<>(List.of(-5, -2, -1, 0, 1, 2, 5, 100, -100, Integer.MAX_VALUE, Integer.MIN_VALUE));
      for (int x : ints) for (int y : ints) for (int z : ints) {
         line("nearest_int", x + " " + y + " " + z, dir(Direction.getNearest(x, y, z, null)) + " " + dir(Direction.getNearest(x, y, z, Direction.UP)));
      }
      List<Double> approx = new ArrayList<>();
      for (double v : new double[]{0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.0E-45, -1.0E-45, 1.0E-50, 3.3, -7.1}) approx.add(v);
      for (int i = 0; i < 3; i++) approx.add((rnd.nextDouble() - 0.5) * 10);
      for (double x : approx) for (double y : approx) for (double z : approx) {
         line("approx_nearest", d(x) + " " + d(y) + " " + d(z), dir(Direction.getApproximateNearest(x, y, z)) + " "
            + dir(Direction.getApproximateNearest((float) x, (float) y, (float) z)) + " " + dir(Direction.getApproximateNearest(new Vec3(x, y, z))));
      }
      for (float yaw : angles(rnd)) {
         StringBuilder facing = new StringBuilder();
         for (Direction x : Direction.values()) facing.append(b(x.isFacingAngle(yaw)));
         line("facing_angle", f(yaw), facing.toString());
      }
      float[] ang = angles(rnd);
      for (int i = 0; i < ang.length; i++) {
         float pitch = ang[i];
         for (int j = 0; j < ang.length; j += 13) {
            float yaw = ang[(j + i * 3) % ang.length];
            Entity e = entity(pitch, yaw);
            StringBuilder order = new StringBuilder();
            for (Direction x : Direction.orderedByNearest(e)) order.append(x.getName()).append(',');
            line("ordered_by_nearest", f(pitch) + " " + f(yaw), order + " " + dir(Direction.getFacingAxis(e, Direction.Axis.X)) + " "
               + dir(Direction.getFacingAxis(e, Direction.Axis.Y)) + " " + dir(Direction.getFacingAxis(e, Direction.Axis.Z)));
         }
      }
      for (Vec3 m : new Vec3[]{new Vec3(1, 0, 2), new Vec3(-3, 4, 3), new Vec3(2, 0, 2), new Vec3(-2, 1, -2), new Vec3(0, 0, 0), new Vec3(-0.0, 0, 0.0)}) {
         StringBuilder order = new StringBuilder();
         for (Direction.Axis a : Direction.axisStepOrder(m)) order.append(a.getName());
         line("axis_step_order", v3(m), order.toString());
      }
      for (long seed : new long[]{0L, 1L, 42L, -7L, 123456789012345L}) {
         RandomSource r = RandomSource.create(seed);
         StringBuilder sb = new StringBuilder();
         for (int i = 0; i < 6; i++) sb.append(Direction.getRandom(r).getName()).append(',');
         for (int i = 0; i < 3; i++) sb.append(Direction.Axis.getRandom(r).getName()).append(',');
         for (Direction.Plane p : Direction.Plane.values()) {
            for (int i = 0; i < 3; i++) sb.append(p.getRandomDirection(r).getName()).append(',').append(p.getRandomAxis(r).getName()).append(',');
            for (Direction x : p.shuffledCopy(r)) sb.append(x.getName()).append(',');
         }
         for (Direction x : Direction.allShuffled(r)) sb.append(x.getName()).append(',');
         sb.append(' ').append(r.nextInt());
         line("dir_random", "" + seed, sb.toString());
      }
   }

   static void axisCycle() {
      for (AxisCycle c : AxisCycle.values()) {
         line("cycle_inverse", c.name(), c.inverse().name());
         for (Direction.Axis a : Direction.Axis.values()) {
            line("cycle", c.name() + " " + a.getName(), ax(c.cycle(a)) + " " + c.cycle(10, 20, 30, a) + " " + d(c.cycle(1.5, 2.5, 3.5, a)));
         }
      }
      for (Direction.Axis a : Direction.Axis.values())
         for (Direction.Axis b : Direction.Axis.values()) line("cycle_between", a.getName() + " " + b.getName(), AxisCycle.between(a, b).name());
   }

   static void composite() {
      CompositeDirection[] cs = {CompositeDirection.NORTH, CompositeDirection.EAST, CompositeDirection.SOUTH, CompositeDirection.WEST,
         CompositeDirection.NORTH_EAST, CompositeDirection.SOUTH_EAST, CompositeDirection.SOUTH_WEST, CompositeDirection.NORTH_WEST,
         CompositeDirection.UP, CompositeDirection.NORTH_UP, CompositeDirection.EAST_UP, CompositeDirection.SOUTH_UP, CompositeDirection.WEST_UP,
         CompositeDirection.NORTH_EAST_UP, CompositeDirection.SOUTH_EAST_UP, CompositeDirection.SOUTH_WEST_UP, CompositeDirection.NORTH_WEST_UP,
         CompositeDirection.DOWN, CompositeDirection.NORTH_DOWN, CompositeDirection.EAST_DOWN, CompositeDirection.SOUTH_DOWN, CompositeDirection.WEST_DOWN,
         CompositeDirection.NORTH_EAST_DOWN, CompositeDirection.SOUTH_EAST_DOWN, CompositeDirection.SOUTH_WEST_DOWN, CompositeDirection.NORTH_WEST_DOWN};
      String[] names = {"NORTH", "EAST", "SOUTH", "WEST", "NORTH_EAST", "SOUTH_EAST", "SOUTH_WEST", "NORTH_WEST", "UP", "NORTH_UP", "EAST_UP", "SOUTH_UP", "WEST_UP",
         "NORTH_EAST_UP", "SOUTH_EAST_UP", "SOUTH_WEST_UP", "NORTH_WEST_UP", "DOWN", "NORTH_DOWN", "EAST_DOWN", "SOUTH_DOWN", "WEST_DOWN",
         "NORTH_EAST_DOWN", "SOUTH_EAST_DOWN", "SOUTH_WEST_DOWN", "NORTH_WEST_DOWN"};
      for (int i = 0; i < cs.length; i++) {
         StringBuilder sb = new StringBuilder();
         for (Direction x : cs[i].directions()) sb.append(x.getName()).append(',');
         line("composite", names[i], sb + " " + cs[i].getStepX() + " " + cs[i].getStepY() + " " + cs[i].getStepZ());
      }
      for (CompositeDirection.Direction8 d8 : CompositeDirection.Direction8.values()) {
         StringBuilder sb = new StringBuilder();
         for (Direction x : d8.getDirections()) sb.append(x.getName()).append(',');
         line("direction8", d8.name(), sb + " " + d8.getStepX() + " " + d8.getStepY() + " " + d8.getStepZ());
      }
   }

   static void rotationMirror(Random rnd) {
      for (Rotation r : Rotation.values()) {
         line("rotation", r.name(), r.getSerializedName() + " " + Rotation.BY_ID.apply(r.ordinal()).name());
         for (Rotation o : Rotation.values()) line("rotation_rotated", r.name() + " " + o.name(), r.getRotated(o).name());
         for (Direction x : Direction.values()) line("rotation_dir", r.name() + " " + x.getName(), dir(r.rotate(x)));
         for (int steps : new int[]{4, 8, 16, 360, 3, 1, 12}) {
            StringBuilder sb = new StringBuilder();
            for (int i = -2; i <= steps + 2; i++) sb.append(r.rotate(i, steps)).append(',');
            line("rotation_steps", r.name() + " " + steps, sb.toString());
         }
      }
      for (int id = -9; id <= 9; id++) line("rotation_by_id", "" + id, Rotation.BY_ID.apply(id).name());
      for (Mirror m : Mirror.values()) {
         line("mirror", m.name(), m.getSerializedName());
         for (Direction x : Direction.values()) line("mirror_dir", m.name() + " " + x.getName(), dir(m.mirror(x)) + " " + m.getRotation(x).name());
         for (int steps : new int[]{4, 8, 16, 360, 3, 1, 12}) {
            StringBuilder sb = new StringBuilder();
            for (int i = -2; i <= steps + 2; i++) sb.append(m.mirror(i, steps)).append(',');
            line("mirror_steps", m.name() + " " + steps, sb.toString());
         }
      }
      for (long seed : new long[]{0L, 5L, -99L, 31337L}) {
         RandomSource r = RandomSource.create(seed);
         StringBuilder sb = new StringBuilder();
         for (int i = 0; i < 6; i++) sb.append(Rotation.getRandom(r).name()).append(',');
         for (Rotation x : Rotation.getShuffled(r)) sb.append(x.name()).append(',');
         sb.append(' ').append(r.nextInt());
         line("rotation_random", "" + seed, sb.toString());
      }
   }

   static void vec2(Random rnd) {
      float[] fp = {0.0F, -0.0F, 1.0F, -1.0F, 0.5F, 3.5F, -2.25F, 1.0E-5F, 1.0E-4F, 1.0E-3F, 123.456F, Float.MAX_VALUE, Float.NaN};
      List<Vec2> vs = new ArrayList<>();
      for (int i = 0; i < fp.length - 1; i += 2) vs.add(new Vec2(fp[i], fp[i + 1]));
      for (int i = 0; i < 6; i++) vs.add(new Vec2((rnd.nextFloat() - 0.5F) * 20, (rnd.nextFloat() - 0.5F) * 20));
      for (Vec2 a : vs) {
         line("vec2_unary", v2(a), v2(a.scale(2.5F)) + " " + v2(a.add(0.75F)) + " " + v2(a.normalized()) + " " + f(a.length()) + " " + f(a.lengthSquared()) + " " + v2(a.negated())
            + " " + v2(a.rotate(0.7)) + " " + v2(a.rotate(-3.0)) + " " + v2(a.rotate(100.5)));
         for (Vec2 c : vs) line("vec2_binary", v2(a) + " " + v2(c), f(a.dot(c)) + " " + v2(a.add(c)) + " " + f(a.distanceToSqr(c)) + " " + b(a.equals(c)));
      }
   }

   static List<Vec3> vecPool(Random rnd) {
      List<Vec3> vs = new ArrayList<>();
      vs.add(new Vec3(0, 0, 0));
      vs.add(new Vec3(-0.0, 0.0, -0.0));
      vs.add(new Vec3(1, 0, 0));
      vs.add(new Vec3(0, -1, 0));
      vs.add(new Vec3(1, 2, 3));
      vs.add(new Vec3(-1.5, 2.25, -3.125));
      vs.add(new Vec3(1.0E-7, -1.0E-7, 1.0000001E-7));
      vs.add(new Vec3(0.5, 0.5, 0.5));
      vs.add(new Vec3(1.0E-6, 0, 0));
      vs.add(new Vec3(17.25, -64.5, 1.0E10));
      vs.add(new Vec3(0.3, 2.7, -3.1));
      for (int i = 0; i < 6; i++) vs.add(new Vec3((rnd.nextDouble() - 0.5) * 20, (rnd.nextDouble() - 0.5) * 20, (rnd.nextDouble() - 0.5) * 20));
      return vs;
   }

   static void vec3s(Random rnd) {
      List<Vec3> vs = vecPool(rnd);
      List<Vec3> specials = new ArrayList<>(vs);
      specials.add(new Vec3(Double.NaN, 0, 0));
      specials.add(new Vec3(0, Double.POSITIVE_INFINITY, 0));
      specials.add(new Vec3(0, 0, Double.NEGATIVE_INFINITY));
      specials.add(new Vec3(Double.MIN_VALUE, Double.MAX_VALUE, -Double.MAX_VALUE));
      for (Vec3 a : specials) {
         line("vec3_flags", v3(a), b(a.isFinite()) + " " + b(a.equals(a)) + " " + b(a.equals(new Vec3(a.x, a.y, a.z))) + " " + a.hashCode() + " " + d(a.length()) + " " + d(a.lengthSqr()));
         line("vec3_special_unary", v3(a), v3(a.normalize()) + " " + v3(a.reverse()) + " " + v3(a.horizontal()) + " " + d(a.horizontalDistance()) + " " + d(a.horizontalDistanceSqr()));
      }
      float[] radians = {0.0F, -0.0F, 0.5F, -1.25F, 1.5707964F, 3.1415927F, 6.2831855F, 100.0F, -77.7F, 0.001F};
      double[] scalars = {0.0, 1.0, -1.0, 0.5, 2.5, 1.0E-7, 17.0};
      for (Vec3 a : vs) {
         StringBuilder sb = new StringBuilder();
         sb.append(v3(a.normalize())).append(' ').append(v3(a.reverse())).append(' ').append(v3(a.horizontal())).append(' ').append(d(a.length()))
            .append(' ').append(d(a.lengthSqr())).append(' ').append(d(a.horizontalDistance())).append(' ').append(d(a.horizontalDistanceSqr())).append(' ')
            .append(v3(a.rotateClockwise90())).append(' ').append(v2(a.rotation()));
         line("vec3_unary", v3(a), sb.toString());
         for (float r : radians) line("vec3_rot", v3(a) + " " + f(r), v3(a.xRot(r)) + " " + v3(a.yRot(r)) + " " + v3(a.zRot(r)));
         for (double s : scalars) {
            line("vec3_scalar", v3(a) + " " + d(s), v3(a.scale(s)) + " " + v3(a.add(s)) + " " + v3(a.subtract(s)) + " " + v3(a.multiply(s, -s, 2 * s)) + " " + v3(a.add(s, -s, 2 * s)) + " "
               + v3(a.subtract(s, -s, 2 * s)) + " " + d(a.distanceToSqr(s, -s, 2 * s)) + " " + b(a.closerThan(Vec3.ZERO, s)) + " "
               + v3(a.relative(Direction.UP, s)) + " " + v3(a.relative(Direction.WEST, s)) + " " + v3(a.relative(Direction.SOUTH, s)));
            for (Direction.Axis ax : Direction.Axis.values()) line("vec3_axis", v3(a) + " " + d(s) + " " + ax.getName(), d(a.get(ax)) + " " + v3(a.with(ax, s)));
         }
         for (int mask = 0; mask < 8; mask++) {
            EnumSet<Direction.Axis> set = EnumSet.noneOf(Direction.Axis.class);
            for (Direction.Axis ax : Direction.Axis.values()) if ((mask >> ax.ordinal() & 1) != 0) set.add(ax);
            line("vec3_align", v3(a) + " " + mask, v3(a.align(set)));
         }
         for (Vec3 c : vs) {
            line("vec3_binary", v3(a) + " " + v3(c), v3(a.add(c)) + " " + v3(a.subtract(c)) + " " + v3(a.cross(c)) + " " + d(a.dot(c)) + " " + v3(a.vectorTo(c)) + " "
               + d(a.distanceTo(c)) + " " + d(a.distanceToSqr(c)) + " " + v3(a.multiply(c)) + " " + v3(a.projectedOn(c)) + " " + b(a.equals(c)) + " "
               + b(a.closerThan(c, 3.0)) + " " + b(a.closerThan(c, 2.0, 1.0)) + " " + b(a.closerThan(c, 0.0, 0.0)));
            for (double t : new double[]{0.25, 1.0, -0.5}) line("vec3_lerp", v3(a) + " " + v3(c) + " " + d(t), v3(a.lerp(c, t)));
         }
      }
      float[] rots = {0.0F, 90.0F, -90.0F, 45.5F, 180.0F, -180.0F, 360.0F, 17.3F, -123.4F, 89.9F};
      for (float rx : rots) for (float ry : rots) line("vec3_direction_from_rotation", f(rx) + " " + f(ry), v3(Vec3.directionFromRotation(rx, ry)) + " " + v3(Vec3.directionFromRotation(new Vec2(rx, ry))));
      for (long seed : new long[]{0L, 7L, -3L}) {
         RandomSource r = RandomSource.create(seed);
         Vec3 base = new Vec3(1.5, -2.5, 3.5);
         line("vec3_offset_random", "" + seed, v3(base.offsetRandom(r, 2.5F)) + " " + v3(base.offsetRandomXZ(r, 0.75F)) + " " + r.nextInt());
      }
      BlockPos[] bps = {new BlockPos(0, 0, 0), new BlockPos(1, 2, 3), new BlockPos(-5, 64, 100), new BlockPos(30000000, -64, -30000000)};
      for (BlockPos p : bps) {
         String in = p.getX() + " " + p.getY() + " " + p.getZ();
         line("vec3_at", in, v3(Vec3.atLowerCornerOf(p)) + " " + v3(Vec3.atLowerCornerWithOffset(p, 0.1, -0.2, 0.3)) + " " + v3(Vec3.atCenterOf(p)) + " " + v3(Vec3.atCenterOfWithY(p, 7.5))
            + " " + v3(Vec3.atBottomCenterOf(p)) + " " + v3(Vec3.upFromBottomCenterOf(p, 0.25)) + " " + v3(new Vec3(p)));
      }
   }

   static void aabbs(Random rnd) {
      List<AABB> boxes = new ArrayList<>();
      boxes.add(new AABB(0, 0, 0, 1, 1, 1));
      boxes.add(new AABB(1, 1, 1, 0, 0, 0)); // reversed
      boxes.add(new AABB(-0.0, 0.0, -0.0, 0.0, -0.0, 0.0)); // signed zeros
      boxes.add(new AABB(0.0, 0.0, 0.0, -0.0, -0.0, -0.0));
      boxes.add(new AABB(0.25, 0.0, 0.25, 0.75, 0.5, 0.75));
      boxes.add(new AABB(-2, -3, -4, 5, 6, 7));
      boxes.add(new AABB(0.5, 0.5, 0.5, 0.5, 0.5, 0.5)); // degenerate
      boxes.add(new AABB(0, 0, 0, 1.0E-7, 1.0E-7, 1.0E-7));
      boxes.add(new AABB(10.1, 20.2, 30.3, 10.9, 20.6, 33.3));
      boxes.add(new AABB(0.3, 2.7, -3.1, 1.7, 3.1, -1.1));
      for (int i = 0; i < 6; i++) {
         double[] c = new double[6];
         for (int j = 0; j < 6; j++) c[j] = Math.round((rnd.nextDouble() - 0.5) * 20 * 16) / 16.0 + (j % 2 == 0 ? 0 : rnd.nextDouble());
         boxes.add(new AABB(c[0], c[1], c[2], c[3], c[4], c[5]));
      }
      List<AABB> special = new ArrayList<>(boxes);
      special.add(new AABB(Double.NaN, 0, 0, 1, 1, 1));
      special.add(new AABB(0, Double.NaN, 0, 1, 1, 1));
      special.add(new AABB(0, 0, 0, 1, 1, Double.POSITIVE_INFINITY));
      special.add(new AABB(Double.MIN_VALUE, Double.MAX_VALUE, -Double.MAX_VALUE, 0, 0, 0));
      for (AABB a : special) {
         line("aabb_new", bb(a), a.hasNaN() ? "1" : "0");
         line("aabb_flags", bb(a), b(a.hasNaN()) + " " + a.hashCode() + " " + b(a.equals(new AABB(a.minX, a.minY, a.minZ, a.maxX, a.maxY, a.maxZ))) + " "
            + bb(a.nextDeflated()) + " " + v3(a.getMinPosition()) + " " + v3(a.getMaxPosition()));
         line("aabb_dims", bb(a), d(a.getSize()) + " " + d(a.getXsize()) + " " + d(a.getYsize()) + " " + d(a.getZsize()) + " " + v3(a.getCenter()) + " " + v3(a.getBottomCenter()));
      }
      // raw constructor args (sorting semantics), including NaN and signed zeros
      double[] ctor = {0.0, -0.0, 1.0, -1.0, Double.NaN, Double.POSITIVE_INFINITY, 2.5};
      for (double x : ctor) for (double y : ctor) line("aabb_ctor", d(x) + " " + d(y), bb(new AABB(x, y, x, y, x, y)) + " " + bb(new AABB(y, x, y, x, y, x)));

      Vec3[] deltas = {new Vec3(0, 0, 0), new Vec3(1, 0, 0), new Vec3(-1, 0, 0), new Vec3(0, 2.5, -0.5), new Vec3(-0.0, 0.0, -0.0), new Vec3(0.3, -0.7, 1.1), new Vec3(-4.5, 3.25, -0.125), new Vec3(1.0E-7, -1.0E-7, 20.0)};
      for (AABB a : boxes) {
         for (Direction.Axis ax : Direction.Axis.values()) line("aabb_axis", bb(a) + " " + ax.getName(), d(a.min(ax)) + " " + d(a.max(ax)));
         line("aabb_set", bb(a) + " " + d(0.4) + " " + d(5.5), bb(a.setMinX(0.4)) + " " + bb(a.setMinY(5.5)) + " " + bb(a.setMinZ(0.4)) + " " + bb(a.setMaxX(5.5)) + " " + bb(a.setMaxY(0.4)) + " " + bb(a.setMaxZ(5.5)));
         for (Vec3 dl : deltas) {
            line("aabb_delta", bb(a) + " " + v3(dl), bb(a.contract(dl.x, dl.y, dl.z)) + " " + bb(a.expandTowards(dl)) + " " + bb(a.expandTowards(dl.x, dl.y, dl.z)) + " " + bb(a.inflate(dl.x, dl.y, dl.z)) + " "
               + bb(a.deflate(dl.x, dl.y, dl.z)) + " " + bb(a.inflate(dl.x)) + " " + bb(a.deflate(dl.y)) + " " + bb(a.move(dl)) + " " + bb(a.move(dl.x, dl.y, dl.z))
               + " " + bb(AABB.ofSize(dl, a.getXsize(), a.getYsize(), a.getZsize())) + " " + bb(AABB.unitCubeFromLowerCorner(dl)) + " " + bb(new AABB(dl, dl.add(a.getCenter())))
               + " " + b(a.contains(dl)) + " " + b(a.contains(dl.x, dl.y, dl.z)) + " " + d(a.distanceToSqr(dl)) + " " + b(a.intersects(dl, dl.add(1, 1, 1))) + " " + b(a.intersects(dl.add(1, 1, 1), dl)));
         }
         for (AABB c : boxes) {
            line("aabb_binary", bb(a) + " " + bb(c), bb(a.intersect(c)) + " " + bb(a.minmax(c)) + " " + b(a.intersects(c)) + " " + b(a.intersects(c.minX, c.minY, c.minZ, c.maxX, c.maxY, c.maxZ)) + " " + d(a.distanceToSqr(c)) + " " + b(a.equals(c)));
         }
         for (BlockPos p : new BlockPos[]{new BlockPos(0, 0, 0), new BlockPos(1, 2, 3), new BlockPos(-5, 64, 100), new BlockPos(0, 0, -1)}) {
            line("aabb_pos", bb(a) + " " + p.getX() + " " + p.getY() + " " + p.getZ(), bb(a.move(p)) + " " + b(a.intersects(p)) + " " + bb(new AABB(p)));
         }
      }
      for (BlockPos p : new BlockPos[]{new BlockPos(0, 0, 0), new BlockPos(1, 2, 3), new BlockPos(-5, 64, 100)}) {
         for (BlockPos q : new BlockPos[]{new BlockPos(0, 0, 0), new BlockPos(-1, 5, 2)}) {
            line("aabb_full_blocks", p.getX() + " " + p.getY() + " " + p.getZ() + " " + q.getX() + " " + q.getY() + " " + q.getZ(), bb(AABB.encapsulatingFullBlocks(p, q)));
         }
      }
      for (int[] bx : new int[][]{{0, 0, 0, 0, 0, 0}, {-3, 4, 5, 10, 20, 30}, {Integer.MAX_VALUE - 1, 0, 0, Integer.MAX_VALUE, 1, 1}}) {
         line("aabb_of_box", bx[0] + " " + bx[1] + " " + bx[2] + " " + bx[3] + " " + bx[4] + " " + bx[5], bb(AABB.of(new BoundingBox(bx[0], bx[1], bx[2], bx[3], bx[4], bx[5]))));
      }

      // clip: rays from pool vectors plus rays aimed through every box face
      List<Vec3[]> generic = new ArrayList<>();
      List<Vec3> vs = vecPool(rnd).subList(0, 9);
      for (Vec3 f : vs) for (Vec3 t : vs) generic.add(new Vec3[]{f, t});
      Map<AABB, List<Vec3[]>> aimed = new IdentityHashMap<>();
      for (AABB a : boxes) {
         List<Vec3[]> rays = new ArrayList<>();
         Vec3 c = a.getCenter();
         for (Direction x : Direction.values()) {
            Vec3 n = x.getUnitVec3();
            rays.add(new Vec3[]{c.add(n.scale(7.0)), c.subtract(n.scale(7.0))});
            rays.add(new Vec3[]{c.add(n.scale(7.0)).add(0.2, 0.1, -0.3), c.subtract(n.scale(7.0))});
            rays.add(new Vec3[]{new Vec3(a.minX, a.minY, a.minZ).add(n.scale(2)), new Vec3(a.minX, a.minY, a.minZ).subtract(n.scale(2))});
            rays.add(new Vec3[]{new Vec3(a.maxX + 1.0E-8, a.maxY, a.maxZ).add(n.scale(2)), new Vec3(a.maxX + 1.0E-8, a.maxY, a.maxZ).subtract(n.scale(2))});
            rays.add(new Vec3[]{new Vec3(a.minX - 1.0E-7, c.y, c.z).add(n), new Vec3(a.minX - 1.0E-7, c.y, c.z).subtract(n)});
            rays.add(new Vec3[]{c.add(n.scale(1.0E-7)), c.subtract(n.scale(1.0E-7))});
            rays.add(new Vec3[]{c.add(n.scale(2.0E-7)), c.subtract(n.scale(2.0E-7))});
         }
         aimed.put(a, rays);
      }
      for (AABB a : boxes) {
         List<Vec3[]> rays = new ArrayList<>(generic);
         rays.addAll(aimed.get(a));
         for (Vec3[] r : rays) {
            Optional<Vec3> res = a.clip(r[0], r[1]);
            Optional<Vec3> res2 = AABB.clip(a.minX, a.minY, a.minZ, a.maxX, a.maxY, a.maxZ, r[0], r[1]);
            line("aabb_clip", bb(a) + " " + v3(r[0]) + " " + v3(r[1]), v3(res.orElse(null)) + " " + v3(res2.orElse(null)));
         }
      }
      List<List<AABB>> groups = List.of(
         List.of(boxes.get(0), boxes.get(4)), List.of(boxes.get(4), boxes.get(0), boxes.get(8)), List.of(boxes.get(5)), List.of(), List.of(boxes.get(1), boxes.get(7), boxes.get(9)),
         List.of(boxes.get(10), boxes.get(11), boxes.get(12)), List.of(boxes.get(13), boxes.get(14), boxes.get(15)));
      int gi = 0;
      for (List<AABB> g : groups) {
         StringBuilder gs = new StringBuilder("" + g.size());
         for (AABB a : g) gs.append(' ').append(bb(a));
         for (BlockPos p : new BlockPos[]{new BlockPos(0, 0, 0), new BlockPos(1, -2, 3)}) {
            List<Vec3[]> rays = new ArrayList<>(generic);
            if (!g.isEmpty()) rays.addAll(aimed.get(g.get(0)));
            for (Vec3[] r : rays) {
               BlockHitResult h = AABB.clip(g, r[0], r[1], p);
               String out = h == null ? "null" : v3(h.getLocation()) + " " + h.getDirection().getName() + " " + h.getBlockPos().getX() + " " + h.getBlockPos().getY() + " " + h.getBlockPos().getZ()
                  + " " + b(h.isInside()) + " " + b(h.getType() == net.minecraft.world.phys.HitResult.Type.MISS) + " " + b(h.isWorldBorderHit());
               line("aabb_clip_boxes", gs + " " + p.getX() + " " + p.getY() + " " + p.getZ() + " " + v3(r[0]) + " " + v3(r[1]), out);
            }
         }
         for (AABB a : boxes) {
            for (Vec3 dl : deltas) line("aabb_collided", bb(a) + " " + v3(dl) + " " + gs, b(a.collidedAlongVector(dl, g)));
         }
      }
      {
         // AABB.Builder
         AABB.Builder bld = new AABB.Builder();
         line("aabb_builder", "start", b(bld.isDefined()));
         float[][] pts = {{1.5F, -2.0F, 3.25F}, {-0.0F, 0.0F, 7.0F}, {0.0F, -0.0F, -7.5F}, {100.0F, 100.0F, 100.0F}};
         StringBuilder in = new StringBuilder();
         for (float[] p : pts) {
            bld.include(new org.joml.Vector3f(p[0], p[1], p[2]));
            in.append(f(p[0])).append(' ').append(f(p[1])).append(' ').append(f(p[2])).append(' ');
            line("aabb_builder_step", in.toString().trim(), b(bld.isDefined()) + " " + bb(bld.build()));
         }
      }
   }
}
