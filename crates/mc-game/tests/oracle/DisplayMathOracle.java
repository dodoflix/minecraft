import com.mojang.math.Transformation;
import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.FloatTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.nbt.TagParser;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.Brightness;
import net.minecraft.util.ExtraCodecs;
import net.minecraft.world.entity.Display;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.joml.Quaternionfc;
import org.joml.Vector3fc;

/**
 * Display transformation math oracle: the real vanilla {@code Transformation} (JOML {@code Matrix4f}/{@code Quaternionf},
 * {@code MatrixUtil.svdDecompose}) and the codecs behind the display entities' data, in process, printing the raw float bits.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/display_math.txt crates/mc-game/tests/oracle/DisplayMathOracle.java
 * Output: crates/mc-game/tests/fixtures/display_math.txt, one case per line: {@code OP TAB input... TAB => TAB result}.
 * The inputs are SNBT (decoded through NbtOps), every float is the hex of {@code Float.floatToRawIntBits}.
 *
 * <pre>
 * DEC snbt              Transformation.EXTENDED_CODEC decode: ERR message | OK P=properties M=16 row-major... decomposed parts
 * CODEC snbt            Transformation.CODEC decode (same result forms)
 * COMPOSE a TAB b       decode both, a.compose(b)
 * PARENT a TAB b        decode both, Transformation.compose(a.getMatrix(), Optional.of(b)) as a matrix
 * INVERSE a             decode, inverse(): NULL | transformation
 * IDINV                 Transformation.IDENTITY.inverse()
 * QUAT snbt             ExtraCodecs.QUATERNIONF decode: ERR message | OK x y z w
 * MAT snbt              ExtraCodecs.MATRIX4F decode: ERR message | OK P=properties M=16 E=the 16 floats re-encoded
 * BR snbt               Brightness.CODEC decode: ERR message | OK block sky packed
 * BILL snbt             Display.BillboardConstraints.CODEC decode: ERR message | OK id
 * BILLID n              Display.BillboardConstraints.BY_ID.apply(n): id
 * </pre>
 * A transformation result is {@code OK P=<matrix properties> M=<the 16 floats row-major> T=<3> L=<4> S=<3> R=<4> E=<14 floats
 * of the CODEC encoding: translation left_rotation scale right_rotation>}; the matrix is printed through getRowColumn.
 */
public final class DisplayMathOracle {
   static final Random R = new Random(0x5EEDL);
   static final List<String> POOL = new ArrayList<>();
   static final StringBuilder OUT = new StringBuilder();

   public static void main(String[] args) {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      handcrafted();
      randomTransforms();
      composePairs();
      inverses();
      parents();
      quaternions();
      matrices();
      brightness();
      billboards();
      codecOnly();
      OUT.append("IDINV\t=>\t").append(dumpOrNull(Transformation.IDENTITY.inverse())).append('\n');
      System.out.print(OUT);
   }

   // ------------------------------------------------------------------------------------------------------ formatting

   static String hex(float f) {
      return String.format("%08x", Float.floatToRawIntBits(f));
   }

   static String f(float v) {
      return Float.toString(v) + "f";
   }

   static String list(float... v) {
      StringBuilder b = new StringBuilder("[");
      for (int i = 0; i < v.length; i++) b.append(i == 0 ? "" : ",").append(f(v[i]));
      return b.append("]").toString();
   }

   static String quat(float x, float y, float z, float w) {
      return list(x, y, z, w);
   }

   static String axisAngle(float angle, float x, float y, float z) {
      return "{angle:" + f(angle) + ",axis:" + list(x, y, z) + "}";
   }

   static String components(String t, String l, String s, String r) {
      return "{translation:" + t + ",left_rotation:" + l + ",scale:" + s + ",right_rotation:" + r + "}";
   }

   /** The matrix as the row-major list of {@code ExtraCodecs.MATRIX4F}. */
   static String matrixList(Matrix4fc m) {
      float[] v = new float[16];
      for (int i = 0; i < 16; i++) v[i] = m.getRowColumn(i >> 2, i & 3);
      return list(v);
   }

   static Tag parse(String snbt) {
      try {
         return TagParser.create(NbtOps.INSTANCE).parseFully(snbt);
      } catch (Exception e) {
         throw new IllegalStateException("bad generated SNBT " + snbt, e);
      }
   }

   static <T> DataResult<T> decode(Codec<T> codec, String snbt) {
      return codec.parse(NbtOps.INSTANCE, parse(snbt));
   }

   static String hexAll(float... v) {
      StringBuilder b = new StringBuilder();
      for (int i = 0; i < v.length; i++) b.append(i == 0 ? "" : " ").append(hex(v[i]));
      return b.toString();
   }

   static String matrix(Matrix4fc m) {
      float[] v = new float[16];
      for (int i = 0; i < 16; i++) v[i] = m.getRowColumn(i >> 2, i & 3);
      return "P=" + m.properties() + " M=" + hexAll(v);
   }

   static String quatBits(Quaternionfc q) {
      return hexAll(q.x(), q.y(), q.z(), q.w());
   }

   static String vecBits(Vector3fc v) {
      return hexAll(v.x(), v.y(), v.z());
   }

   static String encoded(Transformation t) {
      DataResult<Tag> r = Transformation.EXTENDED_CODEC.encodeStart(NbtOps.INSTANCE, t);
      if (r.result().isEmpty()) return "ERR";
      CompoundTag tag = (CompoundTag)r.result().get();
      StringBuilder b = new StringBuilder();
      for (String key : new String[] {"translation", "left_rotation", "scale", "right_rotation"}) {
         for (Tag e : (ListTag)tag.get(key)) b.append(b.length() == 0 ? "" : " ").append(hex(((FloatTag)e).floatValue()));
      }
      return b.toString();
   }

   static String dump(Transformation t) {
      return "OK " + matrix(t.getMatrix()) + " T=" + vecBits(t.translation()) + " L=" + quatBits(t.leftRotation()) + " S=" + vecBits(t.scale())
         + " R=" + quatBits(t.rightRotation()) + " E=" + encoded(t);
   }

   static String dumpOrNull(Transformation t) {
      return t == null ? "NULL" : dump(t);
   }

   static String result(DataResult<Transformation> r) {
      return r.result().isPresent() ? dump(r.result().get()) : "ERR " + r.error().get().message();
   }

   static void line(String op, String input, String result) {
      OUT.append(op).append('\t').append(input).append("\t=>\t").append(result).append('\n');
   }

   static void dec(String snbt) {
      DataResult<Transformation> r = decode(Transformation.EXTENDED_CODEC, snbt);
      if (r.result().isPresent()) POOL.add(snbt);
      line("DEC", snbt, result(r));
   }

   static Transformation decoded(String snbt) {
      return decode(Transformation.EXTENDED_CODEC, snbt).result().get();
   }

   // ------------------------------------------------------------------------------------------------------ transformations

   static float rnd(float range) {
      return (R.nextFloat() * 2.0F - 1.0F) * range;
   }

   static String rndVec(float range) {
      return list(rnd(range), rnd(range), rnd(range));
   }

   static String rndQuatList() {
      return quat(rnd(1.0F), rnd(1.0F), rnd(1.0F), rnd(1.0F));
   }

   static String rndAxisAngle() {
      return axisAngle(rnd(8.0F), rnd(1.0F), rnd(1.0F), rnd(1.0F));
   }

   static String rndRotation() {
      return R.nextBoolean() ? rndQuatList() : rndAxisAngle();
   }

   static void handcrafted() {
      Matrix4f id = new Matrix4f();
      dec(matrixList(id));
      dec(matrixList(new Matrix4f().translation(1.0F, 2.0F, 3.0F)));
      dec(matrixList(new Matrix4f().translation(-0.5F, 1.0E10F, 3.0E-10F)));
      for (float[] s : new float[][] {{2, 3, 4}, {-1, 1, 1}, {-1, -1, -1}, {1, -1, 1}, {0, 1, 1}, {1, 0, 1}, {1, 1, 0}, {0, 0, 0}, {1.0E-3F, 1, 1},
         {1, 1.0E-6F, 1}, {1, 1, 1.0E-7F}, {1.0E-4F, 1.0E-4F, 1.0E-4F}, {1.0E20F, 1, 1}, {3.0E38F, 3.0E38F, 3.0E38F}, {0.5F, 0.5F, 0.5F}, {1, 2, 1}}) {
         dec(matrixList(new Matrix4f().scaling(s[0], s[1], s[2])));
         dec(components(list(0, 0, 0), quat(0, 0, 0, 1), list(s[0], s[1], s[2]), quat(0, 0, 0, 1)));
      }
      // rotations as matrices
      for (float deg : new float[] {0, 30, 45, 90, 135, 180, 270, 359, -90, 1, 0.001F}) {
         float a = (float)Math.toRadians(deg);
         dec(matrixList(new Matrix4f().rotationX(a)));
         dec(matrixList(new Matrix4f().rotationY(a)));
         dec(matrixList(new Matrix4f().rotationZ(a)));
         dec(matrixList(new Matrix4f().rotation(a, 1.0F, 2.0F, 3.0F)));
         dec(components(list(0, 0, 0), axisAngle(a, 0, 1, 0), list(1, 1, 1), quat(0, 0, 0, 1)));
         dec(components(list(1, 2, 3), axisAngle(a, 1, 2, 3), list(1, 2, 3), axisAngle(-a, 0, 0, 1)));
      }
      // angles outside [0, 2*PI) and degenerate axes
      for (float angle : new float[] {0, 1, 3.14159265F, 6.2831855F, 7, -1, -7, 1.0E10F, -1.0E10F, 1.0E-20F, Float.MIN_VALUE}) {
         dec(components(list(0, 0, 0), axisAngle(angle, 0, 0, 1), list(1, 1, 1), quat(0, 0, 0, 1)));
         dec(components(list(0, 0, 0), axisAngle(angle, 0, 0, 0), list(1, 1, 1), quat(0, 0, 0, 1)));
         dec(components(list(0, 0, 0), axisAngle(angle, 3, 4, 5), list(1, 1, 1), axisAngle(angle, -1, 1, 0.5F)));
      }
      // quaternion component forms: unnormalised, zero (NaN), tiny, huge, -0.0
      for (float[] q : new float[][] {{0, 0, 0, 1}, {0, 0, 0, 2}, {0, 0, 0, 0}, {1, 1, 1, 1}, {1, 2, 3, 4}, {-1, 0, 0, 0}, {0, 0, 0, -1}, {1.0E-30F, 0, 0, 0},
         {1.0E30F, 1.0E30F, 0, 0}, {3.0E38F, 3.0E38F, 3.0E38F, 3.0E38F}, {-0.0F, -0.0F, -0.0F, 1}, {0.70710678F, 0, 0.70710678F, 0}, {0.1F, 0.2F, 0.3F, 0.4F}}) {
         dec(components(list(0, 0, 0), quat(q[0], q[1], q[2], q[3]), list(1, 1, 1), quat(0, 0, 0, 1)));
         dec(components(list(5, 6, 7), quat(0, 0, 0, 1), list(2, 2, 2), quat(q[0], q[1], q[2], q[3])));
         dec(components(list(5, 6, 7), quat(q[0], q[1], q[2], q[3]), list(1, 2, 3), quat(q[3], q[2], q[1], q[0])));
      }
      // shear, reflections, permutations, rank deficient, projective and affine-but-not-orthonormal matrices
      Matrix4f shear = new Matrix4f();
      shear.m10(1.0F);
      dec(matrixList(shear));
      shear = new Matrix4f();
      shear.m01(0.5F).m21(-2.0F).m02(3.0F);
      dec(matrixList(shear));
      dec(matrixList(new Matrix4f(0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(1, 2, 3, 0, 2, 4, 6, 0, 3, 6, 9, 0, 1, 1, 1, 1)));
      dec(matrixList(new Matrix4f(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)));
      dec(matrixList(new Matrix4f(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 2)));
      dec(matrixList(new Matrix4f(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1, 2, 3, 4)));
      dec(matrixList(new Matrix4f(1, 0, 0, 0.1F, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, -1, 0, 0, 0, 0)));
      dec(matrixList(new Matrix4f().perspective(1.0F, 1.5F, 0.1F, 100.0F)));
      dec(matrixList(new Matrix4f().ortho(-1, 1, -1, 1, -1, 1)));
      dec(matrixList(new Matrix4f(2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 4, 0, 5, 6, 7, 8)));
      dec(matrixList(new Matrix4f(Float.MAX_VALUE, 0, 0, 0, 0, Float.MAX_VALUE, 0, 0, 0, 0, Float.MAX_VALUE, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(3.0E38F, 3.0E38F, 0, 0, 3.0E38F, 3.0E38F, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(1.0E-30F, 0, 0, 0, 0, 1.0E-30F, 0, 0, 0, 0, 1.0E-30F, 0, 0, 0, 0, 1)));
      dec(matrixList(new Matrix4f(-0.0F, -0.0F, -0.0F, -0.0F, -0.0F, 1, -0.0F, -0.0F, -0.0F, -0.0F, 1, -0.0F, -0.0F, -0.0F, -0.0F, 1)));
      dec(matrixList(new Matrix4f(1, -0.0F, 0, 0, 0, 1, -0.0F, 0, -0.0F, 0, 1, 0, -0.0F, -0.0F, -0.0F, 1)));
      // other numeric tag types and invalid shapes
      dec("[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]");
      dec("[1.0d,0.0d,0.0d,0.0d,0.0d,1.0d,0.0d,0.0d,0.0d,0.0d,1.0d,0.0d,1.5d,2.5d,3.5d,1.0d]");
      dec("[1s,0s,0s,0s,0s,1s,0s,0s,0s,0s,1s,0s,0s,0s,0s,1s]");
      dec("[1b,0b,0b,0b,0b,1b,0b,0b,0b,0b,1b,0b,0b,0b,0b,1b]");
      dec("[1L,0L,0L,0L,0L,1L,0L,0L,0L,0L,1L,0L,0L,0L,0L,1L]");
      dec("[1.0f,0.0f,0.0f]");
      dec("[1.0f,0.0f,0.0f,0.0f,0.0f,1.0f,0.0f,0.0f,0.0f,0.0f,1.0f,0.0f,0.0f,0.0f,0.0f,1.0f,1.0f]");
      dec("[]");
      dec("[\"a\",\"b\"]");
      dec("1.0f");
      dec("\"x\"");
      dec("{}");
      dec("{translation:[0.0f,0.0f,0.0f]}");
      dec("{translation:[0.0f,0.0f],left_rotation:[0.0f,0.0f,0.0f,1.0f],scale:[1.0f,1.0f,1.0f],right_rotation:[0.0f,0.0f,0.0f,1.0f]}");
      dec("{translation:[0.0f,0.0f,0.0f],left_rotation:[0.0f,0.0f,0.0f],scale:[1.0f,1.0f,1.0f],right_rotation:[0.0f,0.0f,0.0f,1.0f]}");
      dec("{translation:[0.0f,0.0f,0.0f],left_rotation:{angle:1.0f},scale:[1.0f,1.0f,1.0f],right_rotation:[0.0f,0.0f,0.0f,1.0f]}");
      dec("{translation:[0.0f,0.0f,0.0f],left_rotation:{axis:[0.0f,1.0f,0.0f]},scale:[1.0f,1.0f,1.0f],right_rotation:[0.0f,0.0f,0.0f,1.0f]}");
      dec("{translation:[0.0f,0.0f,0.0f],left_rotation:\"q\",scale:[1.0f,1.0f,1.0f],right_rotation:[0.0f,0.0f,0.0f,1.0f]}");
      dec(components(list(1, 2, 3), quat(0, 0, 0, 1), list(1, 1, 1), quat(0, 0, 0, 1)).replace("}", ",extra:1b}"));
      dec("{translation:[1,2,3],left_rotation:[0,0,0,1],scale:[1,1,1],right_rotation:[0,0,0,1]}");
      dec("{translation:[1.0d,2.0d,3.0d],left_rotation:{angle:1.0d,axis:[0,1,0]},scale:[1,1,1],right_rotation:[0,0,0,1]}");
      dec("{translation:[1.0f,2.0f,3.0f,4.0f],left_rotation:[0,0,0,1],scale:[1,1,1],right_rotation:[0,0,0,1]}");
   }

   static void randomTransforms() {
      // components with unnormalised quaternion lists and axis-angle objects
      for (int i = 0; i < 140; i++) {
         float range = new float[] {1.0F, 10.0F, 1000.0F}[R.nextInt(3)];
         String scale = switch (R.nextInt(5)) {
            case 0 -> list(rnd(3.0F), rnd(3.0F), rnd(3.0F));
            case 1 -> list(Math.abs(rnd(3.0F)), Math.abs(rnd(3.0F)), Math.abs(rnd(3.0F)));
            case 2 -> list(rnd(1.0E-3F), rnd(2.0F), rnd(2.0F));
            case 3 -> list(1.0F, rnd(1.0F), Math.abs(rnd(1.0E-4F)));
            default -> list(Math.abs(rnd(0.5F)), Math.abs(rnd(0.5F)), Math.abs(rnd(0.5F)));
         };
         dec(components(rndVec(range), rndRotation(), scale, rndRotation()));
      }
      // pure translation / pure scale / one rotation with the other parts omitted by identity values
      for (int i = 0; i < 20; i++) dec(components(rndVec(50.0F), quat(0, 0, 0, 1), list(1, 1, 1), quat(0, 0, 0, 1)));
      for (int i = 0; i < 20; i++) dec(components(list(0, 0, 0), rndRotation(), list(1, 1, 1), quat(0, 0, 0, 1)));
      for (int i = 0; i < 20; i++) dec(components(list(0, 0, 0), quat(0, 0, 0, 1), list(1, 1, 1), rndRotation()));
      // random affine and projective matrices
      for (int i = 0; i < 120; i++) {
         float[] v = new float[16];
         for (int k = 0; k < 16; k++) v[k] = rnd(2.0F);
         v[12] = v[13] = v[14] = 0;
         v[15] = 1;
         dec(list(v));
      }
      for (int i = 0; i < 40; i++) {
         float[] v = new float[16];
         for (int k = 0; k < 16; k++) v[k] = rnd(2.0F);
         dec(list(v));
      }
      // sheared and rank deficient matrices built from rotations, scales and a shear
      for (int i = 0; i < 80; i++) {
         Matrix4f m = new Matrix4f().translation(rnd(5.0F), rnd(5.0F), rnd(5.0F));
         m.rotate(rnd(3.0F), rnd(1.0F), rnd(1.0F), rnd(1.0F) + 0.01F);
         Matrix4f shear = new Matrix4f();
         shear.setRowColumn(R.nextInt(3), R.nextInt(3), rnd(2.0F));
         m.mul(shear);
         float[] s = {rnd(2.0F), rnd(2.0F), rnd(2.0F)};
         if (i % 4 == 0) s[R.nextInt(3)] = 0.0F;
         if (i % 7 == 0) s[R.nextInt(3)] = rnd(1.0E-5F);
         m.scale(s[0], s[1], s[2]);
         m.rotate(rnd(3.0F), rnd(1.0F) + 0.01F, rnd(1.0F), rnd(1.0F));
         dec(matrixList(m));
      }
      // random matrices with zeroed columns and duplicated columns
      for (int i = 0; i < 40; i++) {
         float[] v = new float[16];
         for (int k = 0; k < 16; k++) v[k] = rnd(2.0F);
         v[12] = v[13] = v[14] = 0;
         v[15] = 1;
         int col = R.nextInt(3);
         for (int row = 0; row < 3; row++) {
            if (i % 3 == 0) v[row * 4 + col] = 0;
            else if (i % 3 == 1) v[row * 4 + col] = v[row * 4 + (col + 1) % 3];
            else v[row * 4 + col] = v[row * 4 + (col + 1) % 3] * 1.0E-4F;
         }
         dec(list(v));
      }
   }

   static String pick() {
      return POOL.get(R.nextInt(POOL.size()));
   }

   /** The five property classes a product can start from: identity, translation, affine, perspective-like, general. */
   static String[] propertyClasses() {
      return new String[] {
         matrixList(new Matrix4f()),
         matrixList(new Matrix4f().translation(1, 2, 3)),
         matrixList(new Matrix4f().rotation(1.0F, 0, 1, 0).scale(2, 3, 4).translate(1, 1, 1)),
         matrixList(new Matrix4f().perspective(1.0F, 1.5F, 0.1F, 100.0F)),
         "[1.0f,2.0f,3.0f,4.0f,5.0f,6.0f,7.0f,8.0f,9.0f,10.0f,11.0f,12.0f,13.0f,14.0f,15.0f,16.0f]",
      };
   }

   static void composePairs() {
      String[] classes = propertyClasses();
      for (String a : classes) {
         for (String b : classes) {
            line("COMPOSE", a + "\t" + b, result(DataResult.success(decoded(a).compose(decoded(b)))));
         }
      }
      for (int i = 0; i < 100; i++) {
         String a = pick(), b = pick();
         line("COMPOSE", a + "\t" + b, result(DataResult.success(decoded(a).compose(decoded(b)))));
      }
      for (int i = 0; i < 20; i++) {
         String a = classes[R.nextInt(classes.length)], b = pick();
         line("COMPOSE", a + "\t" + b, result(DataResult.success(decoded(a).compose(decoded(b)))));
         line("COMPOSE", b + "\t" + a, result(DataResult.success(decoded(b).compose(decoded(a)))));
      }
   }

   static void inverses() {
      for (String a : propertyClasses()) line("INVERSE", a, dumpOrNull(decoded(a).inverse()));
      for (String a : POOL) {
         if (R.nextInt(3) == 0) line("INVERSE", a, dumpOrNull(decoded(a).inverse()));
      }
   }

   static void parents() {
      String[] classes = propertyClasses();
      for (String a : classes) {
         for (String b : classes) {
            line("PARENT", a + "\t" + b, matrix(Transformation.compose(decoded(a).getMatrix(), Optional.of(decoded(b)))));
         }
      }
      for (int i = 0; i < 30; i++) {
         String a = pick(), b = pick();
         line("PARENT", a + "\t" + b, matrix(Transformation.compose(decoded(a).getMatrix(), Optional.of(decoded(b)))));
      }
   }

   // ------------------------------------------------------------------------------------------------------ small codecs

   static void quaternions() {
      List<String> inputs = new ArrayList<>(List.of(
         quat(0, 0, 0, 1), quat(0, 0, 0, 0), quat(1, 2, 3, 4), quat(0, 0.70710678F, 0, 0.70710678F), quat(1.0E-30F, 0, 0, 0),
         quat(3.0E38F, 3.0E38F, 0, 0), quat(-0.0F, -0.0F, -0.0F, -0.0F), "[1.0f,2.0f,3.0f]", "[1.0f,2.0f,3.0f,4.0f,5.0f]", "[]", "1.0f", "\"q\"", "{}",
         "[1,2,3,4]", "[1.0d,2.0d,3.0d,4.0d]", "[0b,0b,0b,1b]",
         axisAngle(0, 0, 1, 0), axisAngle(1.5707964F, 0, 1, 0), axisAngle(3.1415927F, 1, 0, 0), axisAngle(-1, 0, 0, 1), axisAngle(7, 1, 1, 1),
         axisAngle(1, 0, 0, 0), axisAngle(1.0E10F, 1, 2, 3), "{angle:1.0f}", "{axis:[0.0f,1.0f,0.0f]}", "{angle:1.0f,axis:[0.0f,1.0f]}",
         "{angle:\"x\",axis:[0.0f,1.0f,0.0f]}", "{angle:1,axis:[0,1,0]}", "{angle:1.0d,axis:[0.0d,1.0d,0.0d]}"));
      for (int i = 0; i < 40; i++) inputs.add(rndQuatList());
      for (int i = 0; i < 40; i++) inputs.add(rndAxisAngle());
      for (String in : inputs) {
         DataResult<Quaternionfc> r = decode(ExtraCodecs.QUATERNIONF, in);
         line("QUAT", in, r.result().isPresent() ? "OK " + quatBits(r.result().get()) : "ERR " + r.error().get().message());
      }
   }

   static void matrices() {
      List<String> inputs = new ArrayList<>(List.of(
         matrixList(new Matrix4f()), matrixList(new Matrix4f().translation(1, 2, 3)), matrixList(new Matrix4f().scaling(2, 3, 4)),
         matrixList(new Matrix4f().rotation(1.0F, 1, 2, 3)), matrixList(new Matrix4f().perspective(1.0F, 1.5F, 0.1F, 100.0F)),
         "[1.0f]", "[]", "5", "{}", "[\"a\"]",
         "[0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f,0.0f]",
         "[1.0f,0.0f,0.0f,0.0f,0.0f,1.0f,0.0f,0.0f,0.0f,0.0f,1.0f,0.0f,0.0f,0.0f,0.0f,1.0f,0.0f]"));
      for (int i = 0; i < 20; i++) {
         float[] v = new float[16];
         for (int k = 0; k < 16; k++) v[k] = rnd(3.0F);
         inputs.add(list(v));
      }
      for (String in : inputs) {
         DataResult<Matrix4fc> r = decode(ExtraCodecs.MATRIX4F, in);
         String out;
         if (r.result().isPresent()) {
            Matrix4fc m = r.result().get();
            ListTag enc = (ListTag)ExtraCodecs.MATRIX4F.encodeStart(NbtOps.INSTANCE, m).result().get();
            StringBuilder b = new StringBuilder();
            for (Tag e : enc) b.append(b.length() == 0 ? "" : " ").append(hex(((FloatTag)e).floatValue()));
            out = "OK " + matrix(m) + " E=" + b;
         } else {
            out = "ERR " + r.error().get().message();
         }
         line("MAT", in, out);
      }
   }

   static void brightness() {
      List<String> inputs = new ArrayList<>(List.of(
         "{block:0,sky:0}", "{block:15,sky:15}", "{block:7,sky:3}", "{block:16,sky:0}", "{block:0,sky:16}", "{block:-1,sky:0}", "{block:0,sky:-1}",
         "{block:16,sky:16}", "{block:5}", "{sky:5}", "{}", "5", "[1,2]", "{block:1.5f,sky:2}", "{block:1.0f,sky:2.0f}", "{block:\"1\",sky:2}",
         "{block:3b,sky:4s}", "{block:3L,sky:4L}", "{block:2147483648L,sky:0}", "{block:1,sky:2,extra:3}"));
      for (int i = 0; i < 30; i++) inputs.add("{block:" + (R.nextInt(24) - 4) + ",sky:" + (R.nextInt(24) - 4) + "}");
      for (String in : inputs) {
         DataResult<Brightness> r = decode(Brightness.CODEC, in);
         line("BR", in, r.result().isPresent() ? "OK " + r.result().get().block() + " " + r.result().get().sky() + " " + r.result().get().pack()
            : "ERR " + r.error().get().message());
      }
   }

   static void billboards() {
      List<String> inputs = List.of("\"fixed\"", "\"vertical\"", "\"horizontal\"", "\"center\"", "\"FIXED\"", "\"\"", "\"diagonal\"", "0", "1", "{}", "[]", "1.0f");
      for (String in : inputs) {
         DataResult<Display.BillboardConstraints> r = decode(Display.BillboardConstraints.CODEC, in);
         line("BILL", in, r.result().isPresent() ? "OK " + r.result().get().ordinal() : "ERR " + r.error().get().message());
      }
      for (int n = -3; n <= 6; n++) line("BILLID", Integer.toString(n), Integer.toString(Display.BillboardConstraints.BY_ID.apply(n).ordinal()));
   }

   static void codecOnly() {
      for (int i = 0; i < 12; i++) {
         String in = components(rndVec(5.0F), rndQuatList(), rndVec(2.0F), rndQuatList());
         line("CODEC", in, result(decode(Transformation.CODEC, in)));
      }
      for (String in : new String[] {matrixList(new Matrix4f()), axisAngle(1, 0, 1, 0), "{translation:[0,0,0]}", "[]",
         components(list(0, 0, 0), axisAngle(1, 0, 1, 0), list(1, 1, 1), quat(0, 0, 0, 1)),
         components(list(0, 0, 0), "[1,2,3]", list(1, 1, 1), quat(0, 0, 0, 1))}) {
         line("CODEC", in, result(decode(Transformation.CODEC, in)));
      }
   }
}
