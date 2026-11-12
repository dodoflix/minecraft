import java.io.PrintStream;
import net.minecraft.core.component.DataComponents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.MapItem;
import net.minecraft.world.level.saveddata.maps.MapItemSavedData;

/**
 * Biome preview map oracle: the real vanilla {@code MapItem.renderBiomePreviewMap} over the seeded biome source of a normal
 * world ({@link VanillaLevel}, seed {@link #SEED}); the colors of each map are printed after the render.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/MapPreviewGolden.java -o crates/mc-game/tests/fixtures/map_preview_golden.txt
 * Output: crates/mc-game/tests/fixtures/map_preview_golden.txt
 *
 * <pre>
 * CASE i scale x z      the map: scale and the block position the map was created at
 * COLORS i hex          the 16384 color bytes after the render
 * </pre>
 */
public final class MapPreviewGolden {
   static final long SEED = 4242L;

   public static void main(String[] args) throws Exception {
      PrintStream out = VanillaLevel.OUT;
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "normal";
      o.seed = SEED;
      o.preloadRadiusChunks = 1;
      VanillaLevel v = VanillaLevel.boot(o);
      ServerLevel level = v.level;
      // scale x z: the first is inside the preloaded chunks' map area, the others mostly reach unloaded chunks
      int[][] cases = {
         {0, 0, 0},
         {0, 500, 500},
         {1, -1000, 300},
         {1, 77, -45},
         {2, 2000, -2000},
         {3, 0, 4000},
         {4, -8000, 6000},
         {4, 3000, 100},
      };
      for (int i = 0; i < cases.length; i++) {
         int scale = cases[i][0], x = cases[i][1], z = cases[i][2];
         ItemStack map = MapItem.create(level, x, z, (byte)scale, true, true);
         MapItem.renderBiomePreviewMap(level, map);
         MapItemSavedData data = level.getMapData(map.get(DataComponents.MAP_ID));
         out.println("CASE " + i + " " + scale + " " + x + " " + z);
         out.println("COLORS " + i + " " + hex(data.colors));
      }
      out.flush();
      System.exit(0);
   }

   static String hex(byte[] bytes) {
      StringBuilder sb = new StringBuilder(bytes.length * 2);
      for (byte b : bytes) {
         sb.append(Character.forDigit((b >> 4) & 15, 16)).append(Character.forDigit(b & 15, 16));
      }
      return sb.toString();
   }
}
