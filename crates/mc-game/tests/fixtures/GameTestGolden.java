import net.minecraft.core.BlockPos;
import net.minecraft.core.Vec3i;
import net.minecraft.gametest.framework.StructureUtils;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.levelgen.structure.BoundingBox;

/** Golden for the pure geometry of the game test framework (Bootstrap routes stdout through the logger, so the output goes to a file).
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/gametest_golden.txt crates/mc-game/tests/fixtures/GameTestGolden.java crates/mc-game/tests/fixtures/gametest_golden.txt
 * Output: crates/mc-game/tests/fixtures/gametest_golden.txt */
public class GameTestGolden {
   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      System.setOut(new java.io.PrintStream(new java.io.FileOutputStream(args[0]), false, "UTF-8"));
      int[][] sizes = {{1, 1, 1}, {3, 4, 5}, {5, 5, 5}, {7, 2, 9}, {1, 1, 48}};
      BlockPos[] origins = {new BlockPos(0, 0, 0), new BlockPos(10, 64, 10), new BlockPos(-33, -5, 100)};
      for (BlockPos origin : origins) {
         for (int[] s : sizes) {
            for (Rotation r : Rotation.values()) {
               Vec3i size = new Vec3i(s[0], s[1], s[2]);
               BlockPos far = StructureUtils.getTransformedFarCorner(origin, size, r);
               BoundingBox box = StructureUtils.getStructureBoundingBox(origin, size, r);
               System.out.println(origin.getX() + " " + origin.getY() + " " + origin.getZ() + " " + s[0] + " " + s[1] + " " + s[2] + " " + r.ordinal()
                  + " far " + far.getX() + " " + far.getY() + " " + far.getZ()
                  + " box " + box.minX() + " " + box.minY() + " " + box.minZ() + " " + box.maxX() + " " + box.maxY() + " " + box.maxZ());
            }
         }
      }
      for (int steps = 0; steps < 4; steps++) {
         System.out.println("steps " + steps + " " + StructureUtils.getRotationForRotationSteps(steps).ordinal() + " " + StructureUtils.getRotationStepsForRotation(StructureUtils.getRotationForRotationSteps(steps)));
      }
   }
}
