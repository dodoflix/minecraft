import java.io.PrintStream;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.MapItem;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.saveddata.maps.MapDecoration;
import net.minecraft.world.level.saveddata.maps.MapId;
import net.minecraft.world.level.saveddata.maps.MapItemSavedData;

/**
 * Map rendering oracle: real vanilla {@code MapItem.update} / {@code HoldingPlayer} packets in the in-process vanilla level
 * ({@link VanillaLevel}, void world). Run with
 * <pre>  scripts/entity-oracle.sh crates/mc-game/tests/oracle/MapGolden.java > crates/mc-game/tests/fixtures/map_golden.txt</pre>
 * Each case builds the terrain of {@link #state} in the region [-48, 48)^2 (everything else stays void air), joins a player
 * holding a fresh map at the given position, ticks the server and prints (see tests/map_golden.rs for the reader):
 * <pre>
 * CASE i scale px py pz yaw ticks seed       the parameters
 * PLAYER i x y z yaw                         the joined player
 * PKT i tick hex                             every map_item_data packet the player received, per tick
 * CENTER i x z scale                         saved data center
 * COLORS i hex                               the 16384 color bytes after the last tick
 * DECO i type x y rot                        decorations after the last tick
 * </pre>
 */
public final class MapGolden {
   static final PrintStream out = VanillaLevel.OUT;

   static int mix(int x, int z, int seed) {
      int h = x * 374761393 + z * 668265263 + seed * 1274126177;
      h = (h ^ (h >>> 13)) * 1274126177;
      return h ^ (h >>> 16);
   }

   static final int LO = -48;
   static final int HI = 48;
   static final int Y_MIN = 40;
   static final int Y_MAX = 100;

   /** The block at (x, y, z) of the terrain with the given seed (must stay identical to the Rust test's copy). */
   static BlockState state(int x, int y, int z, int seed) {
      int kind = Math.floorMod(mix(x >> 2, z >> 2, seed), 12);
      int h = 62 + ((mix(x >> 1, z >> 1, seed + 7) & 7) - 3);
      int d = 1 + (mix(x, z, seed + 3) & 7);
      Block b = null;
      switch (kind) {
         case 0, 1 -> b = y >= 50 && y <= h ? Blocks.STONE : null;
         case 2 -> b = y == h ? Blocks.GRASS_BLOCK : y >= h - 3 && y < h ? Blocks.DIRT : null;
         case 3 -> b = y == h ? Blocks.SAND : y >= 50 && y < h ? Blocks.SANDSTONE : null;
         case 4 -> b = y == h - d ? Blocks.SAND : y > h - d && y <= h ? Blocks.WATER : y >= 50 && y < h - d ? Blocks.STONE : null;
         case 5 -> b = y == h ? Blocks.ICE : y >= h - 3 && y < h ? Blocks.WATER : y == h - 4 ? Blocks.SAND : null;
         case 6 -> b = y == h + 1 ? Blocks.SNOW : y == h ? Blocks.SNOW_BLOCK : y >= 50 && y < h ? Blocks.STONE : null;
         case 7 -> b = y == h + 3 ? Blocks.OAK_LEAVES : y >= 50 && y <= h ? Blocks.STONE : null;
         case 8 -> b = y == h ? Blocks.GLASS : y >= 50 && y < h ? Blocks.STONE : null;
         case 9 -> b = y >= h - 2 && y <= h ? Blocks.LAVA : y >= 50 && y < h - 2 ? Blocks.STONE : null;
         case 10 -> b = null;
         default -> b = y >= 50 && y <= h ? Blocks.TERRACOTTA : null;
      }
      return b == null ? Blocks.AIR.defaultBlockState() : b.defaultBlockState();
   }

   static String hex(byte[] bytes) {
      StringBuilder sb = new StringBuilder(bytes.length * 2);
      for (byte b : bytes) {
         sb.append(Character.forDigit((b >> 4) & 15, 16)).append(Character.forDigit(b & 15, 16));
      }
      return sb.toString();
   }

   public static void main(String[] args) throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "void";
      VanillaLevel v = VanillaLevel.boot(o);
      // scale px py pz yaw ticks seed
      int[][] cases = {
         {0, 3, 80, -9, 0, 40, 1},
         {0, -20, 80, 11, 90, 40, 2},
         {1, 5, 80, 5, -45, 48, 3},
         {2, -7, 80, 13, 135, 48, 4},
         {3, 0, 80, 0, 180, 48, 5},
         {4, 30, 80, -30, 10, 48, 6},
      };
      for (int i = 0; i < cases.length; i++) {
         run(v, i, cases[i]);
      }
   }

   static void run(VanillaLevel v, int i, int[] c) throws Exception {
      int scale = c[0], px = c[1], py = c[2], pz = c[3], yaw = c[4], ticks = c[5], seed = c[6];
      ServerLevel level = v.level;
      BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
      for (int x = LO; x < HI; x++) {
         for (int z = LO; z < HI; z++) {
            for (int y = Y_MIN; y < Y_MAX; y++) {
               pos.set(x, y, z);
               BlockState s = state(x, y, z, seed);
               if (level.getBlockState(pos) != s) {
                  level.setBlock(pos, s, 18 | 256 | 512);
               }
            }
         }
      }
      out.println("CASE " + i + " " + scale + " " + px + " " + py + " " + pz + " " + yaw + " " + ticks + " " + seed);
      ItemStack map = MapItem.create(level, px, pz, (byte)scale, true, false);
      VanillaLevel.FakePlayer fp = v.connect("p" + i, GameType.SURVIVAL, px + 0.5, py, pz + 0.5, yaw, 0f, false);
      fp.player.getInventory().setItem(0, map);
      fp.drain();
      out.println("PLAYER " + i + " " + fp.player.getX() + " " + fp.player.getY() + " " + fp.player.getZ() + " " + fp.player.getYRot());
      for (int t = 0; t < ticks; t++) {
         v.tick();
         for (VanillaLevel.Sent sent : fp.drain()) {
            if (sent.type().equals("minecraft:map_item_data")) {
               out.println("PKT " + i + " " + t + " " + sent.hex());
            }
         }
      }
      MapId id = map.get(DataComponents.MAP_ID);
      MapItemSavedData data = level.getMapData(id);
      out.println("CENTER " + i + " " + data.centerX + " " + data.centerZ + " " + data.scale);
      out.println("COLORS " + i + " " + hex(data.colors));
      for (MapDecoration d : data.getDecorations()) {
         out.println("DECO " + i + " " + d.type().unwrapKey().get().identifier() + " " + d.x() + " " + d.y() + " " + d.rot());
      }
   }
}
