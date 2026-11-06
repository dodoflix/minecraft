// Golden generator for crates/mc-game-core/tests/golden/entity_base_golden.rs. Bootstrap routes stdout through the logger, so the output goes to a file.
// Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/entity_base/view_vector.txt crates/mc-game/tests/fixtures/EntityBaseGolden.java crates/mc-game/tests/fixtures/entity_base/view_vector.txt
// Output: crates/mc-game/tests/fixtures/entity_base/view_vector.txt
// Entity.calculateViewVector(xRot, yRot) (public static) over a deterministic grid of angles.
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

public class EntityBaseGolden {
   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      System.setOut(new java.io.PrintStream(new java.io.FileOutputStream(args[0]), false, "UTF-8"));
      long seed = 12345L;
      java.util.Random r = new java.util.Random(seed);
      for (int i = 0; i < 400; i++) {
         float x = i < 100 ? (i - 50) * 1.8F : (r.nextFloat() * 180.0F - 90.0F);
         float y = i < 100 ? (i * 7.3F - 360.0F) : (r.nextFloat() * 720.0F - 360.0F);
         Vec3 v = Entity.calculateViewVector(x, y);
         System.out.println(Float.floatToRawIntBits(x) + " " + Float.floatToRawIntBits(y) + " " + Double.doubleToRawLongBits(v.x) + " " + Double.doubleToRawLongBits(v.y) + " " + Double.doubleToRawLongBits(v.z));
      }
   }
}
