import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import net.minecraft.world.level.levelgen.Beardifier;
import net.minecraft.world.level.levelgen.densityfunction.DensityBuffer;
import net.minecraft.world.level.levelgen.densityfunction.DensityVolume;
import net.minecraft.world.level.levelgen.densityfunction.SamplerContext;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.TerrainAdjustment;
import net.minecraft.world.level.levelgen.structure.pools.JigsawJunction;
import net.minecraft.world.level.levelgen.structure.pools.StructureTemplatePool;

/**
 * Golden generator for mc-worldgen Beardifier tests (synthetic pieces/junctions handed straight to vanilla's Beardifier).
 * Usage: java -cp $(scripts/vanilla-cp.sh) BeardifierOracle.java > tests/fixtures/beardifier.txt
 *
 * Lines: R case adjustment minX minY minZ maxX maxY maxZ groundLevelDelta       (rigid piece)
 *        J case sourceX sourceGroundY sourceZ                                   (junction)
 *        W case minX minY minZ maxX maxY maxZ                                   (affected box; absent = null)
 *        S case x y z bits                                                      (sampleValue)
 *        V case sizeX sizeY sizeZ minX minY minZ stepX stepY stepZ hash          (sampleVolume digest, fnv over float bits)
 *        KERNEL hash                                                            (junction at origin sampled on [-14, 14]^3)
 */
public class BeardifierOracle {
   static long fnv(long h, int word) {
      for (int i = 0; i < 4; i++) {
         h ^= (word >>> (8 * i)) & 0xFF;
         h *= 0x100000001b3L;
      }
      return h;
   }

   static String bits(float f) {
      return String.format("%08x", Float.floatToRawIntBits(f));
   }

   public static void main(String[] args) {
      PrintWriter out = new PrintWriter(System.out);
      Random r = new Random(20260901L);
      TerrainAdjustment[] adjustments = TerrainAdjustment.values();
      for (int c = 0; c < 40; c++) {
         List<Beardifier.Rigid> rigids = new ArrayList<>();
         List<JigsawJunction> junctions = new ArrayList<>();
         BoundingBox all = null;
         int pieces = 1 + r.nextInt(4);
         int cx = r.nextInt(64) - 32;
         int cz = r.nextInt(64) - 32;
         int cy = r.nextInt(120) - 20;
         for (int i = 0; i < pieces; i++) {
            TerrainAdjustment adj = adjustments[r.nextInt(adjustments.length)];
            int minX = cx + r.nextInt(40) - 20;
            int minY = cy + r.nextInt(30) - 15;
            int minZ = cz + r.nextInt(40) - 20;
            BoundingBox box = new BoundingBox(minX, minY, minZ, minX + r.nextInt(30), minY + r.nextInt(20), minZ + r.nextInt(30));
            int delta = r.nextInt(9) - 4;
            rigids.add(new Beardifier.Rigid(box, adj, delta));
            out.println("R " + c + " " + adj.ordinal() + " " + box.minX() + " " + box.minY() + " " + box.minZ() + " " + box.maxX() + " " + box.maxY() + " " + box.maxZ() + " " + delta);
            all = all == null ? box : BoundingBox.encapsulating(all, box);
         }
         int js = r.nextInt(4);
         for (int i = 0; i < js; i++) {
            int sx = cx + r.nextInt(48) - 24;
            int sy = cy + r.nextInt(30) - 15;
            int sz = cz + r.nextInt(48) - 24;
            junctions.add(new JigsawJunction(sx, sy, sz, 0, StructureTemplatePool.Projection.RIGID));
            out.println("J " + c + " " + sx + " " + sy + " " + sz);
            BoundingBox jb = new BoundingBox(new net.minecraft.core.BlockPos(sx, sy, sz));
            all = BoundingBox.encapsulating(all, jb);
         }
         // Case 0..37: the affected box is the inflated union like forStructuresInChunk; 38/39 use nulls / a tiny box.
         BoundingBox affected = c == 38 ? null : (c == 39 ? new BoundingBox(cx, cy, cz, cx + 3, cy + 3, cz + 3) : all.inflatedBy(24));
         if (affected != null) {
            out.println("W " + c + " " + affected.minX() + " " + affected.minY() + " " + affected.minZ() + " " + affected.maxX() + " " + affected.maxY() + " " + affected.maxZ());
         }
         Beardifier b = new Beardifier(List.copyOf(rigids), List.copyOf(junctions), affected);
         SamplerContext ctx = SamplerContext.EMPTY_UNCACHED;
         for (int i = 0; i < 60; i++) {
            Beardifier.Rigid anchor = rigids.get(r.nextInt(rigids.size()));
            BoundingBox ab = anchor.box();
            int x = ab.minX() - 14 + r.nextInt(ab.maxX() - ab.minX() + 29);
            int y = ab.minY() - 14 + r.nextInt(ab.maxY() - ab.minY() + 29);
            int z = ab.minZ() - 14 + r.nextInt(ab.maxZ() - ab.minZ() + 29);
            if (i % 5 == 4 && !junctions.isEmpty()) {
               JigsawJunction jn = junctions.get(r.nextInt(junctions.size()));
               x = jn.getSourceX() + r.nextInt(27) - 13;
               y = jn.getSourceGroundY() + r.nextInt(27) - 13;
               z = jn.getSourceZ() + r.nextInt(27) - 13;
            }
            out.println("S " + c + " " + x + " " + y + " " + z + " " + bits(b.sampleValue(ctx, x, y, z)));
         }
         int[][] vols = {
            {16, 40, 16, cx - 8, cy - 20, cz - 8, 1, 1, 1},
            {5, 8, 5, cx - 20, cy - 16, cz - 20, 4, 8, 4},
            {1, 30, 1, cx, cy - 10, cz, 1, 1, 1},
            {16, 96, 16, (cx - 8) & ~15, -64, (cz - 8) & ~15, 1, 1, 1}
         };
         for (int[] v : vols) {
            DensityVolume vol = new DensityVolume(v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8]);
            DensityBuffer buf = DensityBuffer.createUnpooled(vol.size());
            b.sampleVolume(ctx, buf, vol);
            long h = 0xcbf29ce484222325L;
            for (int i = 0; i < vol.size(); i++) h = fnv(h, Float.floatToRawIntBits(buf.get(i)));
            out.println("V " + c + " " + v[0] + " " + v[1] + " " + v[2] + " " + v[3] + " " + v[4] + " " + v[5] + " " + v[6] + " " + v[7] + " " + v[8] + " " + String.format("%016x", h));
         }
      }
      Beardifier kernel = new Beardifier(List.of(), List.of(new JigsawJunction(0, 0, 0, 0, StructureTemplatePool.Projection.RIGID)), new BoundingBox(-30, -30, -30, 30, 30, 30));
      long h = 0xcbf29ce484222325L;
      for (int z = -14; z <= 14; z++)
         for (int x = -14; x <= 14; x++)
            for (int y = -14; y <= 14; y++)
               h = fnv(h, Float.floatToRawIntBits(kernel.sampleValue(SamplerContext.EMPTY_UNCACHED, x, y, z)));
      out.println("KERNEL " + String.format("%016x", h));
      out.flush();
   }
}
