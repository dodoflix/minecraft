// Golden generator for Block.getStateForPlacement (and the replace-clicked decision of BlockPlaceContext): for every block it
// builds WORLDS x PER_WORLD placement contexts over pseudo-random neighbourhoods in the REAL vanilla level (see VanillaLevel) and prints
// one 64-bit hash per block (or the detail per context with `verbose <block>`, `verbose '*'` for every block).
// Usage: scripts/golden-block-placement.sh   (`verbose <block>` prints the per-context detail instead)
// Output: crates/mc-game/tests/fixtures/block_placement.txt
// A context = clicked cell, clicked face, hit location, player yaw/pitch/sneaking, held item (the block's own), the states of the
// clicked cell and of the cell beside it (air, water, flowing water, lava, snow, grass, a state of the block itself, or the random
// neighbourhood cell), all derived from the block name and the context index. The hashing, the neighbourhood function (the pools of
// BlockBehaviourOracle) and the context derivation are mirrored 1:1 by crates/mc-game/tests/golden/block_placement_golden.rs.
import com.mojang.authlib.GameProfile;
import java.nio.file.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

public class BlockPlacementOracle {
   static final int WORLDS = 4; // neighbourhoods per block
   static final int PER_WORLD = 16; // contexts per neighbourhood
   // blocks some placements need next to them (cocoa: a jungle log, pointed dripstone: dripstone, ...), added to the pool of the
   // block behaviour golden
   static final String[] EXTRA = {"jungle_log", "pointed_dripstone", "redstone_wire", "redstone_block", "rail", "powered_rail", "lever", "stone",
      "oak_fence", "oak_stairs", "scaffolding", "sea_pickle", "snow", "vine", "sculk_vein", "white_bed", "torch", "wall_torch", "oak_door",
      "oak_trapdoor", "oak_sign", "piston", "sticky_piston", "observer", "tripwire_hook", "tripwire"};
   static final int POOL_CAP = 24;
   static final int CLASS_CAP = 200;
   // the neighbourhood is the 8x8x8 cube at BASE; the clicked cell is one of its 8 central cells, so every cell a placement reads
   // (up to two blocks away from the placement position) is inside it
   static final BlockPos BASE = new BlockPos(4, 80, 4);
   static final int SIZE = 8;
   static final int FLAGS = Block.UPDATE_SKIP_ALL_SIDEEFFECTS;
   static final Direction[] FACES = {Direction.DOWN, Direction.UP, Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST};
   static final double[] FRACTION = {0.0, 0.0625, 0.25, 0.4375, 0.5, 0.5625, 0.75, 1.0};
   // selector of the clicked/neighbour cell: 0 keep, 1 air, 2 water, 3 flowing water, 4 own state, 5 snow, 6 short grass, 7 lava
   static final int[] SEL = {0, 0, 1, 1, 2, 3, 4, 4, 4, 5, 6, 7};

   static List<BlockState> global = new ArrayList<>();
   static List<BlockState> own = List.of();
   static List<BlockState> mates = List.of();
   static long worldSeed;
   static ServerLevel level;
   static ServerPlayer player;

   static long mix(long z) {
      z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
      z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
      return z ^ (z >>> 31);
   }

   static long cellHash(int x, int y, int z) {
      return mix(worldSeed * 0x9E3779B97F4A7C15L + ((long)x * 73856093L ^ (long)y * 19349663L ^ (long)z * 83492791L));
   }

   static BlockState cell(int x, int y, int z) {
      long h = cellHash(x, y, z);
      int r = (int)Long.remainderUnsigned(h, 100);
      long h2 = mix(h);
      List<BlockState> pool;
      if (r < 25) return Blocks.AIR.defaultBlockState();
      else if (r < 55) pool = global;
      else if (r < 80) pool = own;
      else pool = mates;
      return pool.get((int)Long.remainderUnsigned(h2, pool.size()));
   }

   static BlockState cell(BlockPos p) {
      return cell(p.getX(), p.getY(), p.getZ());
   }

   static List<BlockState> capped(Block b, int cap) {
      List<BlockState> all = b.getStateDefinition().getPossibleStates();
      return all.size() <= cap ? all : all.subList(0, cap);
   }

   static String name(Block b) {
      return BuiltInRegistries.BLOCK.getKey(b).getPath();
   }

   static long fnv(String s) {
      long h = 0xCBF29CE484222325L;
      for (byte c : s.getBytes(java.nio.charset.StandardCharsets.UTF_8)) h = (h ^ (c & 0xFF)) * 0x100000001B3L;
      return h;
   }

   static void put(BlockPos p, BlockState s) {
      if (level.getBlockState(p) != s) level.setBlock(p, s, FLAGS);
   }

   static BlockState special(int sel, BlockState kept, long ownIndex) {
      return switch (sel) {
         case 1 -> Blocks.AIR.defaultBlockState();
         case 2 -> Blocks.WATER.defaultBlockState();
         case 3 -> Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL, 3);
         case 4 -> own.get((int)Long.remainderUnsigned(ownIndex, own.size()));
         case 5 -> Blocks.SNOW.defaultBlockState();
         case 6 -> Blocks.SHORT_GRASS.defaultBlockState();
         case 7 -> Blocks.LAVA.defaultBlockState();
         default -> kept;
      };
   }

   static String str(BlockState s) {
      return s == null ? "none" : BlockStateParser.serialize(s);
   }

   public static void main(String[] args) throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "void";
      o.preloadRadiusChunks = 2;
      VanillaLevel v = VanillaLevel.boot(o);
      level = v.level;
      player = new ServerPlayer(v.server, level, new GameProfile(UUID.nameUUIDFromBytes("placement".getBytes()), "placement"), ClientInformation.createDefault());
      for (String n : Files.readAllLines(Path.of(args[0]))) {
         if (n.isBlank()) continue;
         Block b = BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(n));
         global.addAll(capped(b, POOL_CAP));
      }
      for (String n : EXTRA) global.addAll(capped(BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(n)), POOL_CAP));
      String only = args.length > 2 && args[1].equals("verbose") ? args[2] : null;
      StringBuilder out = new StringBuilder();
      for (Block b : BuiltInRegistries.BLOCK) {
         String nm = name(b);
         if (only != null && !only.equals("*") && !only.equals(nm)) continue;
         own = b.getStateDefinition().getPossibleStates();
         List<BlockState> m = new ArrayList<>();
         for (Block other : BuiltInRegistries.BLOCK) {
            if (other.getClass() == b.getClass() && m.size() < CLASS_CAP) m.addAll(capped(other, CLASS_CAP - m.size()));
         }
         mates = m;
         long nameHash = fnv(nm);
         Item item = b.asItem();
         StringBuilder verbose = only != null ? new StringBuilder() : null;
         long h = 0;
         for (int k = 0; k < WORLDS * PER_WORLD; k++) {
            if (k % PER_WORLD == 0) {
               worldSeed = mix(nameHash ^ (k / PER_WORLD + 1));
               for (int x = 0; x < SIZE; x++) for (int y = 0; y < SIZE; y++) for (int z = 0; z < SIZE; z++) {
                  BlockPos p = BASE.offset(x, y, z);
                  put(p, cell(p));
               }
            }
            long r0 = mix(nameHash + k);
            long r1 = mix(r0);
            long r2 = mix(r1);
            BlockPos clicked = BASE.offset(3 + (int)(r0 & 1), 3 + (int)((r0 >>> 1) & 1), 3 + (int)((r0 >>> 2) & 1));
            Direction face = FACES[(int)Long.remainderUnsigned(r0 >>> 3, 6)];
            float yaw = -180.0f + 22.5f * (int)((r0 >>> 12) & 15);
            float pitch = -90.0f + 22.5f * (int)Long.remainderUnsigned(r0 >>> 16, 9);
            boolean sneak = ((r0 >>> 40) & 1) != 0;
            boolean inside = ((r0 >>> 41) & 1) != 0;
            double fx = FRACTION[(int)(r1 & 7)], fy = FRACTION[(int)((r1 >>> 3) & 7)], fz = FRACTION[(int)((r1 >>> 6) & 7)];
            int cSel = SEL[(int)Long.remainderUnsigned(r1 >>> 9, 12)];
            int pSel = SEL[(int)Long.remainderUnsigned(r1 >>> 20, 12)];
            BlockPos beside = clicked.relative(face);
            BlockState cState = special(cSel, cell(clicked), r2 & 0xFFFFFFFFL);
            BlockState pState = special(pSel, cell(beside), r2 >>> 32);
            put(clicked, cState);
            put(beside, pState);
            double hx = clicked.getX() + (face.getStepX() != 0 ? (face.getStepX() > 0 ? 1.0 : 0.0) : fx);
            double hy = clicked.getY() + (face.getStepY() != 0 ? (face.getStepY() > 0 ? 1.0 : 0.0) : fy);
            double hz = clicked.getZ() + (face.getStepZ() != 0 ? (face.getStepZ() > 0 ? 1.0 : 0.0) : fz);
            player.setYRot(yaw);
            player.setYHeadRot(yaw); // a connected player's head follows its body (orderedByNearest reads the head)
            player.setXRot(pitch);
            player.setShiftKeyDown(sneak);
            ItemStack stack = item == Items.AIR ? ItemStack.EMPTY : new ItemStack(item);
            BlockHitResult hit = new BlockHitResult(new Vec3(hx, hy, hz), face, clicked, inside);
            BlockPlaceContext ctx = new BlockPlaceContext(player, InteractionHand.MAIN_HAND, stack, hit);
            level.getRandom().setSeed(mix(r2)); // growing plant heads draw their age from the level random
            BlockState result = b.getStateForPlacement(ctx);
            boolean replaceClicked = ctx.replacingClickedOnBlock();
            boolean canPlace = ctx.canPlace();
            h = mix(h ^ (result == null ? 0 : Block.getId(result) + 1L));
            h = mix(h ^ (replaceClicked ? 1 : 2));
            h = mix(h ^ (canPlace ? 1 : 2));
            if (verbose != null) {
               verbose.append(k).append(" face=").append(face).append(" yaw=").append(yaw).append(" pitch=").append(pitch).append(" sneak=").append(sneak ? 1 : 0)
                  .append(" click=").append(hx).append(',').append(hy).append(',').append(hz).append(" inside=").append(inside ? 1 : 0)
                  .append(" clicked=").append(str(cState)).append(" beside=").append(str(pState)).append(" -> ").append(str(result))
                  .append(" rc=").append(replaceClicked ? 1 : 0).append(" cp=").append(canPlace ? 1 : 0);
               BlockPos q = ctx.getClickedPos();
               for (Direction d : FACES) verbose.append(d.getAxis().isHorizontal() ? " " + d + "=" + str(level.getBlockState(q.relative(d))) : "");
               verbose.append('\n');
            }
            put(clicked, cell(clicked));
            put(beside, cell(beside));
         }
         out.append(nm).append(' ').append(Long.toHexString(h)).append('\n');
         if (verbose != null) out.append(verbose);
      }
      VanillaLevel.OUT.print(out);
      VanillaLevel.OUT.flush();
      System.exit(0);
   }
}
