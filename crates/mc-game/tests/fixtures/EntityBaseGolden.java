// Golden generator: scripts/vanilla-java.sh crates/mc-game/tests/fixtures/EntityBaseGolden.java | sed "s/^.*\[STDOUT\]: //" > crates/mc-game/tests/fixtures/entity_base/view_vector.txt  (Bootstrap routes stdout through the logger)
// Entity.calculateViewVector(xRot, yRot) (public static) over a deterministic grid of angles.
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

public class EntityBaseGolden {
   public static void main(String[] args) {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
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
