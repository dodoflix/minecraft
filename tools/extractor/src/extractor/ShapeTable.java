package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import it.unimi.dsi.fastutil.doubles.DoubleList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.ArrayVoxelShape;
import net.minecraft.world.phys.shapes.CubeVoxelShape;
import net.minecraft.world.phys.shapes.DiscreteVoxelShape;
import net.minecraft.world.phys.shapes.SliceShape;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Interns VoxelShapes by their exact vanilla internals: per-axis coordinate lists, DiscreteVoxelShape size, filled bits
 * and first/last full indices per axis. Shape id 0 is always the empty shape.
 */
public final class ShapeTable {
   private final Map<String, Integer> ids = new HashMap<>();
   private final JsonArray shapes = new JsonArray();

   public int intern(VoxelShape shape) {
      JsonObject o = dump(shape);
      String key = o.toString();
      Integer id = this.ids.get(key);
      if (id == null) {
         id = this.shapes.size();
         this.ids.put(key, id);
         this.shapes.add(o);
      }
      return id;
   }

   public JsonArray table() {
      return this.shapes;
   }

   private static JsonArray doubles(DoubleList l) {
      JsonArray a = new JsonArray();
      for (int i = 0; i < l.size(); i++) {
         a.add(l.getDouble(i));
      }
      return a;
   }

   private static JsonObject dump(VoxelShape shape) {
      DiscreteVoxelShape d = R.get(shape, "shape");
      int xs = d.getXSize();
      int ys = d.getYSize();
      int zs = d.getZSize();
      JsonObject o = new JsonObject();
      o.addProperty("kind", shape instanceof CubeVoxelShape ? "cube" : shape instanceof ArrayVoxelShape ? "array" : shape instanceof SliceShape ? "slice" : shape.getClass().getSimpleName());
      JsonArray size = new JsonArray();
      size.add(xs);
      size.add(ys);
      size.add(zs);
      o.add("size", size);
      JsonArray first = new JsonArray();
      JsonArray last = new JsonArray();
      for (Direction.Axis axis : Direction.Axis.values()) {
         first.add(d.firstFull(axis));
         last.add(d.lastFull(axis));
      }
      o.add("first", first);
      o.add("last", last);
      // bit i = (x * ySize + y) * zSize + z, little-endian within bytes, hex-encoded
      byte[] bytes = new byte[(xs * ys * zs + 7) / 8];
      for (int x = 0; x < xs; x++) {
         for (int y = 0; y < ys; y++) {
            for (int z = 0; z < zs; z++) {
               if (d.isFull(x, y, z)) {
                  int i = (x * ys + y) * zs + z;
                  bytes[i >> 3] |= (byte)(1 << (i & 7));
               }
            }
         }
      }
      StringBuilder hex = new StringBuilder();
      for (byte b : bytes) {
         hex.append(String.format("%02x", b & 0xFF));
      }
      o.addProperty("bits", hex.toString());
      o.add("xs", doubles(shape.getCoords(Direction.Axis.X)));
      o.add("ys", doubles(shape.getCoords(Direction.Axis.Y)));
      o.add("zs", doubles(shape.getCoords(Direction.Axis.Z)));
      JsonArray aabbs = new JsonArray();
      List<AABB> boxes = shape.toAabbs();
      for (AABB b : boxes) {
         JsonArray a = new JsonArray();
         a.add(b.minX);
         a.add(b.minY);
         a.add(b.minZ);
         a.add(b.maxX);
         a.add(b.maxY);
         a.add(b.maxZ);
         aabbs.add(a);
      }
      o.add("aabbs", aabbs);
      return o;
   }
}
