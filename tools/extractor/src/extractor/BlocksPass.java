package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.SupportType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.level.block.state.properties.EnumProperty;
import net.minecraft.world.level.block.state.properties.IntegerProperty;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * blocks.json — see STATUS.md "mc-data extract schema". Block-level values come from Block / BlockBehaviour.Properties,
 * per-state values from BlockStateBase exactly as vanilla evaluates them (shapes with EmptyBlockGetter at BlockPos.ZERO and
 * CollisionContext.empty()).
 */
public final class BlocksPass implements Pass {
   private final Map<Property<?>, Integer> propIds = new IdentityHashMap<>();
   private final Map<Property<?>, String> propConsts = new IdentityHashMap<>();
   private final JsonArray propTable = new JsonArray();
   private final Map<SoundType, Integer> soundIds = new IdentityHashMap<>();
   private final JsonArray soundTable = new JsonArray();
   private int shapeErrors;

   @Override
   public String name() {
      return "blocks";
   }

   private static void flag(JsonObject o, String k, boolean v) {
      if (v) {
         o.addProperty(k, true);
      }
   }

   private static void num(JsonObject o, String k, int v) {
      if (v != 0) {
         o.addProperty(k, v);
      }
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   private static int valueIndex(BlockState s, Property p) {
      return p.getPossibleValues().indexOf(s.getValue(p));
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   private int propId(Property<?> p) {
      Integer id = this.propIds.get(p);
      if (id != null) {
         return id;
      }
      id = this.propTable.size();
      this.propIds.put(p, id);
      JsonObject o = new JsonObject();
      o.addProperty("const", this.propConsts.get(p));
      o.addProperty("name", p.getName());
      JsonArray values = new JsonArray();
      if (p instanceof BooleanProperty) {
         o.addProperty("type", "boolean");
      } else if (p instanceof IntegerProperty) {
         o.addProperty("type", "integer");
      } else if (p instanceof EnumProperty) {
         o.addProperty("type", "enum");
         o.addProperty("enum_class", p.getValueClass().getName());
         o.addProperty("enum_simple_name", p.getValueClass().getSimpleName());
         JsonArray consts = new JsonArray();
         for (Object v : p.getPossibleValues()) {
            consts.add(((Enum<?>)v).name());
         }
         o.add("enum_constants", consts);
         // every constant of the Java enum (a property may expose only a subset), with its serialized name
         JsonArray all = new JsonArray();
         for (Object k : p.getValueClass().getEnumConstants()) {
            JsonObject ko = new JsonObject();
            ko.addProperty("name", ((Enum<?>)k).name());
            ko.addProperty("serialized", ((net.minecraft.util.StringRepresentable)k).getSerializedName());
            all.add(ko);
         }
         o.add("enum_all", all);
      } else {
         throw new IllegalStateException("unknown property kind " + p);
      }
      for (Object v : p.getPossibleValues()) {
         values.add(((Property)p).getName((Comparable)v));
      }
      o.add("values", values);
      this.propTable.add(o);
      return id;
   }

   private int soundId(SoundType t) {
      Integer id = this.soundIds.get(t);
      if (id != null) {
         return id;
      }
      id = this.soundTable.size();
      this.soundIds.put(t, id);
      JsonObject o = new JsonObject();
      o.addProperty("volume", t.volume);
      o.addProperty("pitch", t.pitch);
      o.addProperty("break", t.getBreakSound().location().toString());
      o.addProperty("step", t.getStepSound().location().toString());
      o.addProperty("place", t.getPlaceSound().location().toString());
      o.addProperty("hit", t.getHitSound().location().toString());
      o.addProperty("fall", t.getFallSound().location().toString());
      this.soundTable.add(o);
      return id;
   }

   private void shape(String what, String block, Supplier<VoxelShape> s, JsonArray out) {
      int id;
      try {
         id = ctxShapes.intern(s.get());
      } catch (RuntimeException e) {
         this.shapeErrors++;
         System.err.println("shape error " + block + " " + what + ": " + e);
         id = -1;
      }
      out.add(id);
   }

   private ShapeTable ctxShapes;

   @Override
   public void run(Ctx ctx) throws Exception {
      this.ctxShapes = ctx.shapes;
      // empty shape must be shape id 0
      ctx.shapes.intern(net.minecraft.world.phys.shapes.Shapes.empty());

      for (Field f : BlockStateProperties.class.getFields()) {
         if (Modifier.isStatic(f.getModifiers()) && Property.class.isAssignableFrom(f.getType())) {
            Property<?> p = (Property<?>)f.get(null);
            String prev = this.propConsts.put(p, f.getName());
            if (prev != null) {
               throw new IllegalStateException("property constants alias: " + prev + " / " + f.getName());
            }
         }
      }

      JsonArray blocks = new JsonArray();
      int stateCount = 0;
      for (int bi = 0; bi < BuiltInRegistries.BLOCK.size(); bi++) {
         Block block = BuiltInRegistries.BLOCK.byId(bi);
         BlockBehaviour.Properties pr = block.properties();
         JsonObject b = new JsonObject();
         String name = BuiltInRegistries.BLOCK.getKey(block).toString();
         b.addProperty("id", bi);
         b.addProperty("name", name);
         b.addProperty("class", R.simpleName(block.getClass()));
         JsonArray chain = new JsonArray();
         for (Class<?> c = block.getClass(); c != null && c != Object.class; c = c.getSuperclass()) {
            if (!c.getSimpleName().isEmpty()) {
               chain.add(c.getSimpleName());
            }
         }
         b.add("classes", chain);
         b.addProperty("description_id", block.getDescriptionId());
         b.addProperty("default_state", Block.getId(block.defaultBlockState()));
         JsonArray props = new JsonArray();
         List<Property<?>> plist = new ArrayList<>(block.getStateDefinition().getProperties());
         for (Property<?> p : plist) {
            props.add(propId(p));
         }
         b.add("props", props);

         b.addProperty("destroy_time", block.defaultDestroyTime());
         b.addProperty("explosion_resistance", block.getExplosionResistance());
         b.addProperty("friction", block.getFriction());
         b.addProperty("speed_factor", block.getSpeedFactor());
         b.addProperty("jump_factor", block.getJumpFactor());
         b.addProperty("bounce_restitution", block.getBounceRestitution());
         b.addProperty("fall_distance_reduction", block.getFallDistanceReduction());
         flag(b, "has_collision", (boolean)R.get(block, "hasCollision"));
         flag(b, "requires_correct_tool_for_drops", (boolean)R.get(pr, "requiresCorrectToolForDrops"));
         flag(b, "random_ticks", (boolean)R.get(pr, "isRandomlyTicking"));
         flag(b, "dynamic_shape", block.hasDynamicShape());
         flag(b, "ignited_by_lava", (boolean)R.get(pr, "ignitedByLava"));
         flag(b, "replaceable", (boolean)R.get(pr, "replaceable"));
         flag(b, "can_occlude", (boolean)R.get(pr, "canOcclude"));
         flag(b, "is_air", (boolean)R.get(pr, "isAir"));
         flag(b, "liquid", (boolean)R.get(pr, "liquid"));
         flag(b, "force_solid_on", (boolean)R.get(pr, "forceSolidOn"));
         flag(b, "force_solid_off", (boolean)R.get(pr, "forceSolidOff"));
         flag(b, "spawn_terrain_particles", (boolean)R.get(pr, "spawnTerrainParticles"));
         flag(b, "has_block_entity", block instanceof net.minecraft.world.level.block.EntityBlock);
         b.addProperty("push_reaction", ((Enum<?>)R.get(pr, "pushReaction")).name());
         b.addProperty("instrument", ((Enum<?>)R.get(pr, "instrument")).name());
         b.addProperty("map_color", ((net.minecraft.world.level.material.MapColor)block.defaultMapColor()).id);
         b.addProperty("sound_type", soundId(block.defaultBlockState().getSoundType()));
         b.addProperty("max_horizontal_offset", (float)R.call(block, "getMaxHorizontalOffset"));
         b.addProperty("max_vertical_offset", (float)R.call(block, "getMaxVerticalOffset"));
         var loot = block.getLootTable();
         b.addProperty("loot_table", loot.isPresent() ? loot.get().identifier().toString() : null);
         var item = block.asItem();
         b.addProperty("item", item == net.minecraft.world.item.Items.AIR ? null : BuiltInRegistries.ITEM.getKey(item).toString());
         JsonArray feats = new JsonArray();
         net.minecraft.world.flag.FeatureFlags.REGISTRY.toNames(block.requiredFeatures()).forEach(i -> feats.add(i.toString()));
         b.add("required_features", feats);

         // offset kind: NONE / XZ / XYZ, derived from the offset function's behaviour
         String offset = "NONE";
         BlockState def = block.defaultBlockState();
         if (def.hasOffsetFunction()) {
            offset = "XZ";
            for (int x = 0; x < 64; x++) {
               if (def.getOffset(new BlockPos(x * 7919, 0, x * 104729)).y != 0.0) {
                  offset = "XYZ";
               }
            }
         }
         b.addProperty("offset_type", offset);

         JsonArray states = new JsonArray();
         List<BlockState> all = block.getStateDefinition().getPossibleStates();
         int first = Block.getId(all.get(0));
         b.addProperty("first_state", first);
         b.addProperty("state_count", all.size());
         int defSound = soundId(def.getSoundType());
         int defColor = def.getMapColor(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).id;
         for (int si = 0; si < all.size(); si++) {
            BlockState s = all.get(si);
            if (Block.getId(s) != first + si) {
               throw new IllegalStateException("non-contiguous state ids in " + name);
            }
            // verify mixed-radix layout: last property varies fastest
            int expect = 0;
            for (Property<?> p : plist) {
               expect = expect * p.getPossibleValues().size() + valueIndex(s, p);
            }
            if (expect != si) {
               throw new IllegalStateException("state order mismatch in " + name + " state " + si + " expect " + expect);
            }
            states.add(state(ctx, name, s, defSound, defColor));
            stateCount++;
         }
         b.add("states", states);
         blocks.add(b);
      }

      JsonObject root = new JsonObject();
      root.add("properties", this.propTable);
      root.add("sound_types", this.soundTable);
      root.addProperty("state_count", stateCount);
      root.add("blocks", blocks);
      ctx.write("blocks.json", root);
      if (this.shapeErrors > 0) {
         System.err.println("WARNING: " + this.shapeErrors + " shape evaluation errors");
      }
   }

   private JsonObject state(Ctx ctx, String name, BlockState s, int defSound, int defColor) {
      JsonObject o = new JsonObject();
      var empty = EmptyBlockGetter.INSTANCE;
      var pos = BlockPos.ZERO;
      var cc = CollisionContext.empty();
      num(o, "light", s.getLightEmission());
      num(o, "light_block", s.getLightDampening());
      flag(o, "can_occlude", s.canOcclude());
      flag(o, "solid_render", s.isSolidRender());
      flag(o, "is_air", s.isAir());
      flag(o, "propagates_skylight_down", s.propagatesSkylightDown());
      flag(o, "use_shape_for_light_occlusion", s.useShapeForLightOcclusion());
      flag(o, "is_redstone_conductor", s.isRedstoneConductor(empty, pos));
      flag(o, "is_suffocating", s.isSuffocating(empty, pos));
      flag(o, "is_view_blocking", s.isViewBlocking(empty, pos, new net.minecraft.world.phys.AABB(0, 0, 0, 0, 0, 0)));
      flag(o, "has_large_collision_shape", s.hasLargeCollisionShape());
      flag(o, "requires_correct_tool_for_drops", s.requiresCorrectToolForDrops());
      flag(o, "is_collision_shape_full_block", s.isCollisionShapeFullBlock(empty, pos));
      flag(o, "legacy_solid", s.isSolid());
      flag(o, "random_ticking", s.isRandomlyTicking());
      flag(o, "signal_source", s.isSignalSource());
      flag(o, "has_analog_output_signal", s.hasAnalogOutputSignal());
      flag(o, "emissive_rendering", s.emissiveRendering());
      flag(o, "render_model", s.getRenderShape() == net.minecraft.world.level.block.RenderShape.MODEL);
      flag(o, "can_be_replaced", s.canBeReplaced());
      flag(o, "liquid", s.liquid());
      flag(o, "ignited_by_lava", s.ignitedByLava());
      flag(o, "has_offset", s.hasOffsetFunction());
      o.addProperty("destroy_speed", s.getDestroySpeed(empty, pos));
      float shade = s.getShadeBrightness(empty, pos);
      o.addProperty("shade_brightness", shade);
      int sturdy = 0;
      for (Direction d : Direction.values()) {
         for (SupportType t : SupportType.values()) {
            if (s.isFaceSturdy(empty, pos, d, t)) {
               sturdy |= 1 << (d.ordinal() * SupportType.values().length + t.ordinal());
            }
         }
      }
      num(o, "face_sturdy", sturdy);
      int path = 0;
      for (PathComputationType t : PathComputationType.values()) {
         if (s.isPathfindable(t)) {
            path |= 1 << t.ordinal();
         }
      }
      num(o, "pathfindable", path);
      o.addProperty("fluid_state", Fluid.FLUID_STATE_REGISTRY.getId(s.getFluidState()));
      int snd = soundId(s.getSoundType());
      if (snd != defSound) {
         o.addProperty("sound_type", snd);
      }
      int col = s.getMapColor(empty, pos).id;
      if (col != defColor) {
         o.addProperty("map_color", col);
      }
      JsonArray sh = new JsonArray(); // collision, outline, occlusion, support, interaction, visual
      shape("collision", name, () -> s.getCollisionShape(empty, pos, cc), sh);
      shape("outline", name, () -> s.getShape(empty, pos, cc), sh);
      shape("occlusion", name, () -> s.getOcclusionShape(), sh);
      shape("support", name, () -> s.getBlockSupportShape(empty, pos), sh);
      shape("interaction", name, () -> s.getInteractionShape(empty, pos), sh);
      shape("visual", name, () -> s.getVisualShape(empty, pos, cc), sh);
      o.add("shapes", sh);
      return o;
   }
}
