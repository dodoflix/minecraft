import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.*;

/** Interprets shapes.script against vanilla and prints the result lines that tests/shapes.rs must reproduce. */
public class ShapesOracle {
    static final Map<String, VoxelShape> S = new HashMap<>();
    static final StringBuilder OUT = new StringBuilder();

    static String hx(double d) { return Long.toHexString(Double.doubleToRawLongBits(d)); }

    static class Hasher {
        long h = 0xcbf29ce484222325L;
        void add(long v) { h = (h ^ v) * 0x100000001b3L; }
        void add(double d) { add(Double.doubleToRawLongBits(d)); }
        void add(int i) { add((long) i); }
        public String toString() { return Long.toHexString(h); }
    }

    static BooleanOp op(String n) throws Exception { return (BooleanOp) BooleanOp.class.getField(n).get(null); }

    static Object field(Object o, Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(o);
    }

    static DiscreteVoxelShape discrete(VoxelShape s) throws Exception { return (DiscreteVoxelShape) field(s, VoxelShape.class, "shape"); }

    static int findIndex(VoxelShape s, Direction.Axis a, double c) throws Exception {
        Method m = VoxelShape.class.getDeclaredMethod("findIndex", Direction.Axis.class, double.class);
        m.setAccessible(true);
        return (Integer) m.invoke(s, a, c);
    }

    static String ref(VoxelShape r, VoxelShape... ops) {
        if (r == Shapes.block()) return "block";
        if (r == Shapes.empty()) return "empty";
        for (int i = 0; i < ops.length; i++) if (r == ops[i]) return "op" + i;
        return "new";
    }

    static String dump(String name, VoxelShape s) throws Exception {
        DiscreteVoxelShape d = discrete(s);
        StringBuilder b = new StringBuilder("dump " + name);
        b.append(" kind=").append(s instanceof CubeVoxelShape ? "cube" : "array");
        b.append(" disc=").append(d instanceof BitSetDiscreteVoxelShape ? "bitset" : "sub");
        b.append(" size=").append(d.getXSize()).append(',').append(d.getYSize()).append(',').append(d.getZSize());
        b.append(" first=");
        for (Direction.Axis a : Direction.Axis.values()) b.append(d.firstFull(a)).append(a == Direction.Axis.Z ? "" : ",");
        b.append(" last=");
        for (Direction.Axis a : Direction.Axis.values()) b.append(d.lastFull(a)).append(a == Direction.Axis.Z ? "" : ",");
        b.append(" words=");
        if (d instanceof BitSetDiscreteVoxelShape) {
            long[] w = ((BitSet) field(d, BitSetDiscreteVoxelShape.class, "storage")).toLongArray();
            if (w.length == 0) b.append('-');
            for (int i = 0; i < w.length; i++) b.append(i == 0 ? "" : ",").append(Long.toHexString(w[i]));
        } else b.append('-');
        b.append(" cells=");
        for (int x = 0; x < d.getXSize(); x++) for (int y = 0; y < d.getYSize(); y++) for (int z = 0; z < d.getZSize(); z++) b.append(d.isFull(x, y, z) ? '1' : '0');
        b.append(" ckind=");
        for (Direction.Axis a : Direction.Axis.values()) b.append(s.getCoords(a) instanceof CubePointRange ? 'c' : 'a');
        for (Direction.Axis a : Direction.Axis.values()) {
            b.append(" c").append(a.getName()).append('=');
            var l = s.getCoords(a);
            for (int i = 0; i < l.size(); i++) b.append(i == 0 ? "" : ",").append(hx(l.getDouble(i)));
        }
        return b.toString();
    }

    static void emit(String s) { OUT.append(s).append('\n'); }

    static Direction.Axis axis(String s) { return Direction.Axis.valueOf(s); }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        double[] vals = {-0.3, 0.0, 0.2, 0.5, 0.8, 1.1};
        double[] sizes = {0.25, 0.6, 1.0};
        double[] deltas = {-1.5, -0.5, -0.2, -0.05, 0.0, 0.05, 0.2, 0.5, 1.5};
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            line = line.trim();
            if (line.isEmpty() || line.startsWith("#")) continue;
            String[] t = line.split("\\s+");
            String cmd = t[0];
            try {
                switch (cmd) {
                    case "empty" -> { S.put(t[1], Shapes.empty()); }
                    case "block" -> { S.put(t[1], Shapes.block()); }
                    case "infinity" -> { S.put(t[1], Shapes.INFINITY); }
                    case "box", "create" -> {
                        double[] v = new double[6];
                        for (int i = 0; i < 6; i++) v[i] = Double.parseDouble(t[2 + i]);
                        VoxelShape s = cmd.equals("box") ? Shapes.box(v[0], v[1], v[2], v[3], v[4], v[5]) : Shapes.create(v[0], v[1], v[2], v[3], v[4], v[5]);
                        S.put(t[1], s);
                    }
                    case "or" -> {
                        VoxelShape first = S.get(t[2]);
                        VoxelShape[] tail = new VoxelShape[t.length - 3];
                        for (int i = 3; i < t.length; i++) tail[i - 3] = S.get(t[i]);
                        S.put(t[1], Shapes.or(first, tail));
                    }
                    case "join", "joinu" -> {
                        VoxelShape a = S.get(t[2]), b = S.get(t[3]);
                        VoxelShape r = cmd.equals("join") ? Shapes.join(a, b, op(t[4])) : Shapes.joinUnoptimized(a, b, op(t[4]));
                        S.put(t[1], r);
                        emit("ref " + t[1] + " " + ref(r, a, b));
                    }
                    case "move" -> {
                        VoxelShape a = S.get(t[2]);
                        VoxelShape r = a.move(Double.parseDouble(t[3]), Double.parseDouble(t[4]), Double.parseDouble(t[5]));
                        S.put(t[1], r);
                        emit("ref " + t[1] + " " + ref(r, a));
                    }
                    case "opt" -> { VoxelShape a = S.get(t[2]); VoxelShape r = a.optimize(); S.put(t[1], r); emit("ref " + t[1] + " " + ref(r, a)); }
                    case "single" -> { VoxelShape a = S.get(t[2]); VoxelShape r = a.singleEncompassing(); S.put(t[1], r); emit("ref " + t[1] + " " + ref(r, a)); }
                    case "face" -> {
                        VoxelShape a = S.get(t[2]);
                        VoxelShape r = a.getFaceShape(Direction.valueOf(t[3]));
                        S.put(t[1], r);
                        VoxelShape r2 = a.getFaceShape(Direction.valueOf(t[3]));
                        emit("ref " + t[1] + " " + ref(r, a) + " cached=" + (r == r2));
                    }
                    case "slice" -> { S.put(t[1], new SliceShape(S.get(t[2]), axis(t[3]), Integer.parseInt(t[4]))); }
                    case "findbits" -> {
                        Method m = Shapes.class.getDeclaredMethod("findBits", double.class, double.class);
                        m.setAccessible(true);
                        emit("findbits " + t[1] + " " + t[2] + " " + m.invoke(null, Double.parseDouble(t[1]), Double.parseDouble(t[2])));
                    }
                    case "dump" -> { VoxelShape s = S.get(t[1]); emit(s == null ? "UNDEF " + t[1] : dump(t[1], s)); }
                    case "aabbs" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        StringBuilder b = new StringBuilder("aabbs " + t[1] + " ");
                        for (AABB a : s.toAabbs()) b.append(hx(a.minX)).append(',').append(hx(a.minY)).append(',').append(hx(a.minZ)).append(',').append(hx(a.maxX)).append(',').append(hx(a.maxY)).append(',').append(hx(a.maxZ)).append(';');
                        emit(b.toString());
                    }
                    case "edges" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        Hasher h = new Hasher();
                        int[] n = {0};
                        s.forAllEdges((x1, y1, z1, x2, y2, z2) -> { n[0]++; h.add(x1); h.add(y1); h.add(z1); h.add(x2); h.add(y2); h.add(z2); });
                        emit("edges " + t[1] + " " + n[0] + " " + h);
                    }
                    case "grid" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        for (Direction.Axis ax : Direction.Axis.values()) {
                            Hasher h = new Hasher();
                            for (double x : vals) for (double y : vals) for (double z : vals) for (double sz : sizes) {
                                AABB box = new AABB(x, y, z, x + sz, y + sz, z + sz);
                                for (double d : deltas) h.add(s.collide(ax, box, d));
                            }
                            emit("grid " + t[1] + " " + ax + " " + h);
                        }
                        // Shapes.collide over an iterable
                        Hasher h = new Hasher();
                        List<VoxelShape> list = List.of(s, S.get("slab"), S.get("fence"));
                        for (double x : vals) for (double y : vals) for (double d : deltas) h.add(Shapes.collide(Direction.Axis.Y, new AABB(x, y, 0.3, x + 0.4, y + 0.4, 0.7), list, d));
                        emit("gridmulti " + t[1] + " " + h);
                    }
                    case "faces" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        DiscreteVoxelShape d = discrete(s);
                        Hasher h = new Hasher();
                        int[] n = {0};
                        d.forAllFaces((dir, x, y, z) -> { n[0]++; h.add(dir.ordinal()); h.add(x); h.add(y); h.add(z); });
                        Hasher hi = new Hasher();
                        if (d instanceof BitSetDiscreteVoxelShape bs)
                            for (int x = 0; x < d.getXSize(); x++) for (int y = 0; y < d.getYSize(); y++) for (int z = 0; z < d.getZSize(); z++) hi.add(bs.isInterior(x, y, z) ? 1 : 0);
                        emit("faces " + t[1] + " " + n[0] + " " + h + " " + hi);
                    }
                    case "bounds" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        try {
                            AABB a = s.bounds();
                            emit("bounds " + t[1] + " " + hx(a.minX) + "," + hx(a.minY) + "," + hx(a.minZ) + "," + hx(a.maxX) + "," + hx(a.maxY) + "," + hx(a.maxZ));
                        } catch (UnsupportedOperationException e) { emit("bounds " + t[1] + " ERR"); }
                    }
                    case "point" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        Hasher h = new Hasher();
                        double[] coords = {-0.3, 0.0, 1e-7, 0.2, 0.5, 0.8, 0.9999999, 1.0, 1.1};
                        for (Direction.Axis ax : Direction.Axis.values()) {
                            for (double b : coords) for (double c : coords) { h.add(s.min(ax, b, c)); h.add(s.max(ax, b, c)); }
                            for (double c : coords) h.add(findIndex(s, ax, c));
                            h.add(s.min(ax)); h.add(s.max(ax));
                        }
                        emit("point " + t[1] + " " + h);
                    }
                    case "closest" -> {
                        VoxelShape s = S.get(t[1]);
                        if (s == null) { emit("UNDEF " + t[1]); break; }
                        var r = s.closestPointTo(new Vec3(Double.parseDouble(t[2]), Double.parseDouble(t[3]), Double.parseDouble(t[4])));
                        emit("closest " + t[1] + " " + t[2] + " " + t[3] + " " + t[4] + " " + (r.isPresent() ? hx(r.get().x) + "," + hx(r.get().y) + "," + hx(r.get().z) : "none"));
                    }
                    case "joinne" -> { emit("joinne " + t[1] + " " + t[2] + " " + t[3] + " " + Shapes.joinIsNotEmpty(S.get(t[1]), S.get(t[2]), op(t[3]))); }
                    case "equal" -> { emit("equal " + t[1] + " " + t[2] + " " + Shapes.equal(S.get(t[1]), S.get(t[2]))); }
                    case "faceocc" -> { emit("faceocc " + t[1] + " " + t[2] + " " + Shapes.faceShapeOccludes(S.get(t[1]), S.get(t[2]))); }
                    case "occl" -> {
                        Direction d = Direction.valueOf(t[3]);
                        emit("occl " + t[1] + " " + t[2] + " " + t[3] + " " + Shapes.blockOccludes(S.get(t[1]), S.get(t[2]), d) + " " + Shapes.mergedFaceOccludes(S.get(t[1]), S.get(t[2]), d));
                    }
                    case "clip" -> {
                        VoxelShape s = S.get(t[1]);
                        double[] v = new double[9];
                        for (int i = 0; i < 9; i++) v[i] = Double.parseDouble(t[2 + i]);
                        BlockHitResult r = s.clip(new Vec3(v[0], v[1], v[2]), new Vec3(v[3], v[4], v[5]), new BlockPos((int) v[6], (int) v[7], (int) v[8]));
                        StringBuilder b = new StringBuilder("clip");
                        for (int i = 1; i < t.length; i++) b.append(' ').append(t[i]);
                        if (r == null) b.append(" null");
                        else b.append(' ').append(hx(r.getLocation().x)).append(',').append(hx(r.getLocation().y)).append(',').append(hx(r.getLocation().z)).append(' ').append(r.getDirection()).append(' ').append(r.getBlockPos().getX()).append(',').append(r.getBlockPos().getY()).append(',').append(r.getBlockPos().getZ()).append(' ').append(r.isInside()).append(' ').append(r.getType() == net.minecraft.world.phys.HitResult.Type.MISS);
                        emit(b.toString());
                    }
                    default -> throw new IllegalStateException("unknown command " + cmd);
                }
            } catch (IllegalStateException e) {
                throw e;
            } catch (RuntimeException e) {
                emit("ERR " + line);
            }
        }
        System.out.print(OUT);
    }
}
