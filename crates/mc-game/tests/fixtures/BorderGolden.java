import com.mojang.serialization.*;
import net.minecraft.core.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.border.*;
import net.minecraft.world.phys.*;
import net.minecraft.world.phys.shapes.*;
import net.minecraft.world.level.ChunkPos;
import java.nio.file.*;
import java.util.*;

/** Replays border_ops.txt on the real WorldBorder: `scripts/vanilla-java.sh BorderGolden.java border_ops.txt border.golden`. */
public class BorderGolden {
  static String d(double v) { return Long.toHexString(Double.doubleToLongBits(v)); }
  static String line(WorldBorder b) {
    StringBuilder s = new StringBuilder();
    for (float p : new float[]{0f, 0.5f, 1f}) s.append(d(b.getMinX(p))).append(' ').append(d(b.getMaxX(p))).append(' ').append(d(b.getMinZ(p))).append(' ').append(d(b.getMaxZ(p))).append(" | ");
    s.append(d(b.getSize())).append(' ').append(b.getLerpTime()).append(' ').append(d(b.getLerpTarget())).append(' ').append(d(b.getLerpSpeed())).append(' ').append(b.getStatus()).append(" | ");
    s.append(d(b.getCenterX())).append(' ').append(d(b.getCenterZ())).append(' ').append(d(b.getSafeZone())).append(' ').append(d(b.getDamagePerBlock())).append(' ').append(b.getWarningTime()).append(' ').append(b.getWarningBlocks()).append(' ').append(b.getAbsoluteMaxSize()).append(" | ");
    double[][] pts = {{0, 0}, {10.5, -7.25}, {49.99, 50}, {-100, 3}, {1e9, -1e9}, {b.getMaxX(), 0}, {b.getMinX(), b.getMinZ()}};
    for (double[] p : pts) {
      s.append(b.isWithinBounds(p[0], p[1])).append(',').append(b.isWithinBounds(p[0], p[1], 2.5)).append(',').append(d(b.getDistanceToBorder(p[0], p[1]))).append(',');
      Vec3 c = b.clampVec3ToBound(p[0], 64, p[1]);
      s.append(d(c.x)).append(',').append(d(c.z)).append(',').append(b.clampToBounds(p[0], 64, p[1]).asLong()).append(';');
    }
    s.append(" | ").append(b.isWithinBounds(new AABB(-1, 0, -1, 1, 2, 1))).append(',').append(b.isWithinBounds(new AABB(b.getMaxX() - 1, 0, 0, b.getMaxX() + 1, 2, 1))).append(',').append(b.isWithinBounds(new ChunkPos(0, 0))).append(',').append(b.isWithinBounds(new ChunkPos(1875000, 0))).append(',').append(b.isWithinBounds(new BlockPos(5, 5, 5)));
    s.append(" | ");
    for (AABB bb : b.getCollisionShape().toAabbs()) s.append(d(bb.minX)).append(',').append(d(bb.minY)).append(',').append(d(bb.minZ)).append(',').append(d(bb.maxX)).append(',').append(d(bb.maxY)).append(',').append(d(bb.maxZ)).append(';');
    s.append(" | ").append(WorldBorder.CODEC.encodeStart(NbtOps.INSTANCE, b).getOrThrow());
    return s.toString();
  }
  public static void main(String[] args) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    java.io.PrintStream out = new java.io.PrintStream(new java.io.FileOutputStream(args[1]), false, "UTF-8");
    WorldBorder b = new WorldBorder();
    for (String op : Files.readAllLines(Path.of(args[0]))) {
      String[] t = op.trim().split(" ");
      switch (t[0]) {
        case "new" -> b = t.length == 1 ? new WorldBorder() : new WorldBorder(new WorldBorder.Settings(Double.parseDouble(t[1]), Double.parseDouble(t[2]), Double.parseDouble(t[3]), Double.parseDouble(t[4]), Integer.parseInt(t[5]), Integer.parseInt(t[6]), Double.parseDouble(t[7]), Long.parseLong(t[8]), Double.parseDouble(t[9])));
        case "size" -> b.setSize(Double.parseDouble(t[1]));
        case "center" -> b.setCenter(Double.parseDouble(t[1]), Double.parseDouble(t[2]));
        case "lerp" -> b.lerpSizeBetween(Double.parseDouble(t[1]), Double.parseDouble(t[2]), Long.parseLong(t[3]), Long.parseLong(t[4]));
        case "tick" -> { for (int i = 0; i < Integer.parseInt(t[1]); i++) b.tick(); }
        case "safe" -> b.setSafeZone(Double.parseDouble(t[1]));
        case "damage" -> b.setDamagePerBlock(Double.parseDouble(t[1]));
        case "wt" -> b.setWarningTime(Integer.parseInt(t[1]));
        case "wb" -> b.setWarningBlocks(Integer.parseInt(t[1]));
        case "absmax" -> b.setAbsoluteMaxSize(Integer.parseInt(t[1]));
        case "init" -> b.applyInitialSettings(Long.parseLong(t[1]));
        default -> throw new IllegalStateException(op);
      }
      out.println(op + " => " + line(b));
    }
    out.close();
  }
}
