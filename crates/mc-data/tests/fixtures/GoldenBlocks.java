// Golden values for crates/mc-data tests, straight from the vanilla classes (independent of the extractor passes).
// Usage: scripts/golden-blocks.sh  (runs this with tools/extractor/build + the vanilla classpath)
// Output: crates/mc-data/tests/fixtures/block_golden.json
import com.google.gson.*;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SupportType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

public class GoldenBlocks {
   static JsonArray aabbs(VoxelShape s) {
      JsonArray a = new JsonArray();
      for (AABB b : s.toAabbs()) {
         JsonArray x = new JsonArray();
         x.add(b.minX); x.add(b.minY); x.add(b.minZ); x.add(b.maxX); x.add(b.maxY); x.add(b.maxZ);
         a.add(x);
      }
      return a;
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      List<Integer> ids = new ArrayList<>();
      int blocks = BuiltInRegistries.BLOCK.size();
      for (int i = 0; i < blocks; i += 4) {
         ids.add(Block.getId(BuiltInRegistries.BLOCK.byId(i).defaultBlockState()));
      }
      Random r = new Random(42);
      int total = Block.BLOCK_STATE_REGISTRY.size();
      for (int i = 0; i < 300; i++) {
         ids.add(r.nextInt(total));
      }
      // blocks with special structure: stairs, doors, fences, redstone wire, water/lava, chests, pistons
      for (String n : List.of("oak_stairs", "oak_door", "oak_fence", "redstone_wire", "water", "lava", "chest", "piston", "cauldron", "dripstone_block", "pointed_dripstone", "bamboo", "poppy", "wheat", "fire", "kelp", "nether_wart", "white_bed", "oak_slab", "oak_log", "hopper", "composter", "note_block", "oak_sign", "repeater", "iron_bars", "cobblestone_wall")) {
         Block b = BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(n));
         for (BlockState s : b.getStateDefinition().getPossibleStates()) {
            if (b.getStateDefinition().getPossibleStates().size() <= 32 || r.nextInt(8) == 0) ids.add(Block.getId(s));
         }
      }
      JsonArray out = new JsonArray();
      var empty = EmptyBlockGetter.INSTANCE;
      var pos = BlockPos.ZERO;
      var cc = CollisionContext.empty();
      for (int id : new TreeSet<>(ids)) {
         BlockState s = Block.stateById(id);
         JsonObject o = new JsonObject();
         o.addProperty("id", id);
         o.addProperty("block", BuiltInRegistries.BLOCK.getId(s.getBlock()));
         o.addProperty("string", s.toString());
         JsonArray set = new JsonArray();
         JsonArray cycle = new JsonArray();
         for (Property<?> p : s.getProperties()) {
            for (Object v : p.getPossibleValues()) {
               JsonArray e = new JsonArray();
               e.add(p.getName());
               e.add(((Property)p).getName((Comparable)v));
               e.add(Block.getId(s.setValue((Property)p, (Comparable)v)));
               set.add(e);
            }
            JsonArray c = new JsonArray();
            c.add(p.getName());
            c.add(Block.getId(s.cycle((Property)p)));
            cycle.add(c);
         }
         o.add("set", set);
         o.add("cycle", cycle);
         o.addProperty("light", s.getLightEmission());
         o.addProperty("light_block", s.getLightDampening());
         JsonObject f = new JsonObject();
         f.addProperty("can_occlude", s.canOcclude());
         f.addProperty("solid_render", s.isSolidRender());
         f.addProperty("is_air", s.isAir());
         f.addProperty("propagates_skylight_down", s.propagatesSkylightDown());
         f.addProperty("use_shape_for_light_occlusion", s.useShapeForLightOcclusion());
         f.addProperty("is_redstone_conductor", s.isRedstoneConductor(empty, pos));
         f.addProperty("is_suffocating", s.isSuffocating(empty, pos));
         f.addProperty("is_view_blocking", s.isViewBlocking(empty, pos, new AABB(0, 0, 0, 0, 0, 0)));
         f.addProperty("has_large_collision_shape", s.hasLargeCollisionShape());
         f.addProperty("requires_correct_tool_for_drops", s.requiresCorrectToolForDrops());
         f.addProperty("is_collision_shape_full_block", s.isCollisionShapeFullBlock(empty, pos));
         f.addProperty("is_solid", s.isSolid());
         f.addProperty("is_randomly_ticking", s.isRandomlyTicking());
         f.addProperty("is_signal_source", s.isSignalSource());
         f.addProperty("has_analog_output_signal", s.hasAnalogOutputSignal());
         f.addProperty("can_be_replaced", s.canBeReplaced());
         f.addProperty("liquid", s.liquid());
         f.addProperty("ignited_by_lava", s.ignitedByLava());
         o.add("flags", f);
         o.addProperty("destroy_speed", s.getDestroySpeed(empty, pos));
         o.addProperty("shade_brightness", s.getShadeBrightness(empty, pos));
         o.addProperty("fluid_state", net.minecraft.world.level.material.Fluid.FLUID_STATE_REGISTRY.getId(s.getFluidState()));
         JsonArray sturdy = new JsonArray();
         for (Direction d : Direction.values()) {
            for (SupportType t : SupportType.values()) {
               sturdy.add(s.isFaceSturdy(empty, pos, d, t));
            }
         }
         o.add("face_sturdy", sturdy);
         JsonArray path = new JsonArray();
         for (PathComputationType t : PathComputationType.values()) {
            path.add(s.isPathfindable(t));
         }
         o.add("pathfindable", path);
         JsonObject sh = new JsonObject();
         sh.add("collision", aabbs(s.getCollisionShape(empty, pos, cc)));
         sh.add("outline", aabbs(s.getShape(empty, pos, cc)));
         sh.add("occlusion", aabbs(s.getOcclusionShape()));
         sh.add("support", aabbs(s.getBlockSupportShape(empty, pos)));
         sh.add("interaction", aabbs(s.getInteractionShape(empty, pos)));
         sh.add("visual", aabbs(s.getVisualShape(empty, pos, cc)));
         o.add("shapes", sh);
         out.add(o);
      }
      java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), new GsonBuilder().disableHtmlEscaping().create().toJson(out));
   }
}
