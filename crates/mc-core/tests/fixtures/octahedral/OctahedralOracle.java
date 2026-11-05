import com.mojang.math.OctahedralGroup;
import com.mojang.math.SymmetricGroup3;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.Direction;
import net.minecraft.core.FrontAndTop;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.*;
import org.joml.Vector3f;
import org.joml.Vector3i;

/**
 * Prints the output lines tests/golden/octahedral.rs must reproduce: group tables, rotation of directions/vectors/
 * FrontAndTop, Rotation/Mirror.rotation(), and the shape rotation family driven by octahedral.script.
 */
public class OctahedralOracle {
    static final Map<String, VoxelShape> S = new HashMap<>();
    static final StringBuilder OUT = new StringBuilder();
    static final OctahedralGroup[] G = OctahedralGroup.values();

    static void emit(String s) { OUT.append(s).append('\n'); }

    static String hx(double d) { return Long.toHexString(Double.doubleToRawLongBits(d)); }

    static Object field(Object o, Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(o);
    }

    static String dump(VoxelShape s) throws Exception {
        DiscreteVoxelShape d = (DiscreteVoxelShape) field(s, VoxelShape.class, "shape");
        StringBuilder b = new StringBuilder("kind=" + (s instanceof CubeVoxelShape ? "cube" : "array"));
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

    static String ref(VoxelShape r, VoxelShape src) { return r == src ? "same" : "new"; }

    static void emitShape(String label, VoxelShape r, VoxelShape src) throws Exception {
        emit(label + " ref=" + ref(r, src) + " " + dump(r));
    }

    static OctahedralGroup group(String n) { return OctahedralGroup.valueOf(n); }

    /** Optional trailing "px py pz"; null when absent. */
    static Vec3 point(String[] t, int from) {
        return t.length >= from + 3 ? new Vec3(Double.parseDouble(t[from]), Double.parseDouble(t[from + 1]), Double.parseDouble(t[from + 2])) : null;
    }

    static void tables() {
        for (OctahedralGroup a : G) {
            StringBuilder b = new StringBuilder("compose " + a.ordinal() + ":");
            for (OctahedralGroup c : G) b.append(' ').append(a.compose(c).ordinal());
            emit(b.toString());
        }
        StringBuilder inv = new StringBuilder("inverse:");
        for (OctahedralGroup a : G) inv.append(' ').append(a.inverse().ordinal());
        emit(inv.toString());
        for (OctahedralGroup a : G) {
            StringBuilder b = new StringBuilder("meta " + a.ordinal() + " " + a + " " + a.getSerializedName());
            b.append(" perm=").append(a.permutation().ordinal());
            b.append(" inv=").append(a.inverts(Direction.Axis.X) ? 1 : 0).append(a.inverts(Direction.Axis.Y) ? 1 : 0).append(a.inverts(Direction.Axis.Z) ? 1 : 0);
            float[] m = new float[9];
            a.transformation().get(m);
            b.append(" m=");
            for (int i = 0; i < 9; i++) b.append(i == 0 ? "" : ",").append(Integer.toHexString(Float.floatToRawIntBits(m[i])));
            emit(b.toString());
        }
        emit("alias " + OctahedralGroup.BLOCK_ROT_X_270.ordinal() + " " + OctahedralGroup.BLOCK_ROT_X_180.ordinal() + " " + OctahedralGroup.BLOCK_ROT_X_90.ordinal()
            + " " + OctahedralGroup.BLOCK_ROT_Y_270.ordinal() + " " + OctahedralGroup.BLOCK_ROT_Y_180.ordinal() + " " + OctahedralGroup.BLOCK_ROT_Y_90.ordinal()
            + " " + OctahedralGroup.BLOCK_ROT_Z_270.ordinal() + " " + OctahedralGroup.BLOCK_ROT_Z_180.ordinal() + " " + OctahedralGroup.BLOCK_ROT_Z_90.ordinal());
        for (OctahedralGroup a : G) {
            StringBuilder b = new StringBuilder("rotdir " + a.ordinal() + ":");
            for (Direction d : Direction.values()) b.append(' ').append(a.rotate(d).ordinal());
            emit(b.toString());
        }
        int[][] vecs = {{1, 2, 3}, {-4, 5, 6}, {0, 0, 0}, {7, -8, 9}, {Integer.MIN_VALUE, 1, -1}, {100, -200, 300}};
        for (OctahedralGroup a : G) {
            StringBuilder b = new StringBuilder("rotvec " + a.ordinal() + ":");
            for (int[] v : vecs) {
                Vector3i r = a.rotate(new Vector3i(v[0], v[1], v[2]));
                b.append(' ').append(r.x).append(',').append(r.y).append(',').append(r.z);
            }
            emit(b.toString());
        }
        for (OctahedralGroup a : G) {
            StringBuilder b = new StringBuilder("rotfat " + a.ordinal() + ":");
            for (FrontAndTop f : FrontAndTop.values()) {
                FrontAndTop r = a.rotate(f);
                b.append(' ').append(r == null ? "-" : String.valueOf(r.ordinal()));
            }
            emit(b.toString());
        }
        for (FrontAndTop f : FrontAndTop.values()) {
            emit("fat " + f.ordinal() + " " + f.getSerializedName() + " " + f.front().ordinal() + " " + f.top().ordinal());
        }
        for (Direction f : Direction.values()) {
            StringBuilder b = new StringBuilder("fatlookup " + f.ordinal() + ":");
            for (Direction t : Direction.values()) {
                FrontAndTop r = FrontAndTop.fromFrontAndTop(f, t);
                b.append(' ').append(r == null ? "-" : String.valueOf(r.ordinal()));
            }
            emit(b.toString());
        }
        for (Rotation r : Rotation.values()) emit("rotation " + r.ordinal() + " " + r.rotation().ordinal());
        for (Mirror m : Mirror.values()) emit("mirror " + m.ordinal() + " " + m.rotation().ordinal());
        for (SymmetricGroup3 a : SymmetricGroup3.values()) {
            StringBuilder b = new StringBuilder("sym " + a.ordinal() + " inv=" + a.inverse().ordinal() + " compose=");
            for (SymmetricGroup3 c : SymmetricGroup3.values()) b.append(a.compose(c).ordinal()).append(',');
            b.append(" permute=").append(a.permute(0)).append(a.permute(1)).append(a.permute(2));
            b.append(" axis=");
            for (Direction.Axis x : Direction.Axis.values()) b.append(a.permuteAxis(x).ordinal());
            Vector3i vi = a.permuteVector(new Vector3i(10, 20, 30));
            Vector3f vf = a.permuteVector(new Vector3f(1.5f, -2.5f, 3.5f));
            b.append(" vi=").append(vi.x).append(',').append(vi.y).append(',').append(vi.z);
            b.append(" vf=").append(Float.floatToRawIntBits(vf.x)).append(',').append(Float.floatToRawIntBits(vf.y)).append(',').append(Float.floatToRawIntBits(vf.z));
            float[] m = new float[9];
            a.transformation().get(m);
            b.append(" m=");
            for (int i = 0; i < 9; i++) b.append(i == 0 ? "" : ",").append(Integer.toHexString(Float.floatToRawIntBits(m[i])));
            emit(b.toString());
        }
    }

    static void emitDirMap(String label, Map<Direction, VoxelShape> m, VoxelShape src) throws Exception {
        for (Direction d : Direction.values()) {
            if (m.containsKey(d)) emitShape(label + " " + d.getName(), m.get(d), src);
        }
        emit(label + " keys=" + m.size());
    }

    static void emitAxisMap(String label, Map<Direction.Axis, VoxelShape> m, VoxelShape src) throws Exception {
        for (Direction.Axis a : Direction.Axis.values()) {
            if (m.containsKey(a)) emitShape(label + " " + a.getName(), m.get(a), src);
        }
        emit(label + " keys=" + m.size());
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        tables();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            line = line.trim();
            if (line.isEmpty() || line.startsWith("#")) continue;
            String[] t = line.split("\\s+");
            switch (t[0]) {
                case "empty" -> S.put(t[1], Shapes.empty());
                case "block" -> S.put(t[1], Shapes.block());
                case "box", "create" -> {
                    double[] v = new double[6];
                    for (int i = 0; i < 6; i++) v[i] = Double.parseDouble(t[2 + i]);
                    S.put(t[1], t[0].equals("box") ? Shapes.box(v[0], v[1], v[2], v[3], v[4], v[5]) : Shapes.create(v[0], v[1], v[2], v[3], v[4], v[5]));
                }
                case "or" -> {
                    VoxelShape[] tail = new VoxelShape[t.length - 3];
                    for (int i = 3; i < t.length; i++) tail[i - 3] = S.get(t[i]);
                    S.put(t[1], Shapes.or(S.get(t[2]), tail));
                }
                case "move" -> S.put(t[1], S.get(t[2]).move(Double.parseDouble(t[3]), Double.parseDouble(t[4]), Double.parseDouble(t[5])));
                case "slice" -> S.put(t[1], new SliceShape(S.get(t[2]), Direction.Axis.valueOf(t[3]), Integer.parseInt(t[4])));
                // rot <src> <GROUP> [px py pz]
                case "rot" -> {
                    VoxelShape src = S.get(t[1]);
                    Vec3 p = point(t, 3);
                    VoxelShape r = p == null ? Shapes.rotate(src, group(t[2])) : Shapes.rotate(src, group(t[2]), p);
                    emitShape("rot " + line.substring(4), r, src);
                }
                // rotall <src> [px py pz]: every group
                case "rotall" -> {
                    VoxelShape src = S.get(t[1]);
                    Vec3 p = point(t, 2);
                    for (OctahedralGroup g : G) {
                        VoxelShape r = p == null ? Shapes.rotate(src, g) : Shapes.rotate(src, g, p);
                        emitShape("rotall " + t[1] + " " + g.ordinal() + (p == null ? "" : " pt"), r, src);
                    }
                }
                // disc <src> <GROUP>: DiscreteVoxelShape.rotate directly
                case "disc" -> {
                    VoxelShape src = S.get(t[1]);
                    DiscreteVoxelShape d = (DiscreteVoxelShape) field(src, VoxelShape.class, "shape");
                    DiscreteVoxelShape r = d.rotate(group(t[2]));
                    emitShape("disc " + t[1] + " " + t[2], new CubeVoxelShape(r), new CubeVoxelShape(d));
                }
                // horizaxis|allaxis <src> [px py pz]
                case "horizaxis" -> {
                    VoxelShape src = S.get(t[1]);
                    Vec3 p = point(t, 2);
                    emitAxisMap(line, p == null ? Shapes.rotateHorizontalAxis(src) : Shapes.rotateHorizontalAxis(src, p), src);
                }
                case "allaxis" -> {
                    VoxelShape src = S.get(t[1]);
                    Vec3 p = point(t, 2);
                    emitAxisMap(line, p == null ? Shapes.rotateAllAxis(src) : Shapes.rotateAllAxis(src, p), src);
                }
                // horiz <src> [GROUP|-] [px py pz]
                case "horiz" -> {
                    VoxelShape src = S.get(t[1]);
                    OctahedralGroup g = t.length > 2 && !t[2].equals("-") ? group(t[2]) : null;
                    Vec3 p = point(t, 3);
                    Map<Direction, VoxelShape> m = g == null ? Shapes.rotateHorizontal(src) : p == null ? Shapes.rotateHorizontal(src, g) : Shapes.rotateHorizontal(src, g, p);
                    emitDirMap(line, m, src);
                }
                // all <src> [GROUP|-] [px py pz]  (no group: (north) or (north, center))
                case "all" -> {
                    VoxelShape src = S.get(t[1]);
                    OctahedralGroup g = t.length > 2 && !t[2].equals("-") ? group(t[2]) : null;
                    Vec3 p = point(t, 3);
                    Map<Direction, VoxelShape> m = g != null ? Shapes.rotateAll(src, g, p == null ? new Vec3(0.5, 0.5, 0.5) : p) : p == null ? Shapes.rotateAll(src) : Shapes.rotateAll(src, p);
                    emitDirMap(line, m, src);
                }
                // attach <src> [GROUP]
                case "attach" -> {
                    VoxelShape src = S.get(t[1]);
                    Map<AttachFace, Map<Direction, VoxelShape>> m = t.length > 2 ? Shapes.rotateAttachFace(src, group(t[2])) : Shapes.rotateAttachFace(src);
                    for (AttachFace f : AttachFace.values()) emitDirMap(line + " " + f.getSerializedName(), m.get(f), src);
                    emit(line + " faces=" + m.size());
                }
                default -> throw new IllegalArgumentException(line);
            }
        }
        System.out.print(OUT);
    }
}
