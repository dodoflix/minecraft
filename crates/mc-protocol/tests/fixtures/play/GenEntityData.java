// Golden generator for the entity_data play group: add_entity, set_entity_data (every EntityDataSerializers type),
// level_particles, explode and every ParticleTypes payload.
// Usage: crates/mc-protocol/tests/fixtures/play/gen.sh GenEntityData.java
// Output: crates/mc-protocol/tests/fixtures/play/entitydata.golden
import com.mojang.datafixers.util.Either;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.OptionalInt;
import java.util.UUID;
import net.minecraft.ChatFormatting;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.GlobalPos;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.Rotations;
import net.minecraft.core.particles.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.network.syncher.EntityDataSerializer;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.util.random.Weighted;
import net.minecraft.util.random.WeightedList;
import net.minecraft.world.entity.EntityReference;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.animal.armadillo.Armadillo;
import net.minecraft.world.entity.animal.golem.CopperGolemState;
import net.minecraft.world.entity.animal.sniffer.Sniffer;
import net.minecraft.world.entity.npc.villager.VillagerData;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.WeatheringCopper;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.PositionSource;
import net.minecraft.world.phys.Vec3;
import com.mojang.authlib.GameProfile;
import com.mojang.authlib.properties.Property;
import com.mojang.authlib.properties.PropertyMap;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.ResolvableProfile;

public class GenEntityData {
   static RegistryFriendlyByteBuf buf() { return new RegistryFriendlyByteBuf(Unpooled.buffer(), PlayHarness.access); }

   @SuppressWarnings({"rawtypes", "unchecked"})
   static void sed(String label, EntityDataSerializer s, Object v) {
      PlayHarness.cb("sed_" + label, new ClientboundSetEntityDataPacket(7, List.of(new SynchedEntityData.DataValue(5, s, v))));
   }

   static void particle(String label, ParticleOptions o) {
      PlayHarness.raw("particle_" + label, b -> ParticleTypes.STREAM_CODEC.encode(b, o));
   }

   static UUID uuid(long a, long b) { return new UUID(a, b); }

   static void addEntity(String label, int id, UUID uuid, double x, double y, double z, float xRot, float yRot, EntityType<?> t, int data, Vec3 mv, double head) {
      PlayHarness.cb("add_entity_" + label, new ClientboundAddEntityPacket(id, uuid, x, y, z, xRot, yRot, t, data, mv, head));
   }

   /** Registers `bytes` (packet id + body) as `<label>.in` and records what vanilla's codec makes of it. */
   static void redecode(String label, ByteBuf in) {
      byte[] bytes = new byte[in.readableBytes()];
      in.getBytes(in.readerIndex(), bytes);
      PlayHarness.raw(label + ".in", b -> b.writeBytes(bytes));
      try {
         Packet<?> p = (Packet<?>) PlayHarness.clientbound.codec().decode(Unpooled.wrappedBuffer(bytes));
         PlayHarness.cb(label, p);
      } catch (Throwable t) {
         // vanilla wraps decode failures in "Failed to decode packet '...'": compare the root cause
         Throwable root = t;
         while (root.getCause() != null) root = root.getCause();
         String msg = root.getMessage();
         PlayHarness.raw(label, b -> b.writeBytes(("ERR:" + msg).getBytes(StandardCharsets.UTF_8)));
      }
   }

   static <T> Holder<T> holder(ResourceKey<? extends Registry<T>> key, int id) {
      return PlayHarness.access.lookupOrThrow(key).asHolderIdMap().byIdOrThrow(id);
   }

   static <T> void variant(String label, ResourceKey<? extends Registry<T>> key, EntityDataSerializer<Holder<T>> s) {
      int n = PlayHarness.access.lookupOrThrow(key).size();
      PlayHarness.raw("count_" + label, b -> b.writeVarInt(n));
      for (int id : new int[]{0, 1, n - 1}) sed(label + "_" + id, s, holder(key, id));
   }

   static BlockState stateById(int id) { return Block.stateById(id); }

   static int lastStateId() { return Block.BLOCK_STATE_REGISTRY.size() - 1; }

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);
      PlayHarness.raw("count_block_states", b -> b.writeVarInt(Block.BLOCK_STATE_REGISTRY.size()));
      PlayHarness.raw("count_particle_types", b -> b.writeVarInt(BuiltInRegistries.PARTICLE_TYPE.size()));
      PlayHarness.raw("count_serializers", b -> b.writeVarInt(44));

      // ---- add_entity
      addEntity("zero", 0, uuid(0, 0), 0, 0, 0, 0f, 0f, EntityTypes.PIG, 0, Vec3.ZERO, 0);
      addEntity("basic", 1, uuid(0x0123456789abcdefL, 0xfedcba9876543210L), 1.5, 64.0, -2.25, 90f, 180f, EntityTypes.ZOMBIE, 7, new Vec3(0.1, -0.2, 0.3), 45f);
      addEntity("neg_angles", 300, uuid(-1, -1), -1e7, -64, 1e7, -45f, -90f, EntityTypes.PLAYER, -1, new Vec3(-0.5, 0.0, 0.5), -170.5);
      addEntity("wrap_angles", 70000, uuid(1, 2), 0.1, 0.2, 0.3, 359.9f, 720f, EntityTypes.ARROW, 123456, new Vec3(1.0, 1.0, 1.0), 360);
      addEntity("fast", Integer.MAX_VALUE, uuid(3, 4), Double.MAX_VALUE, Double.MIN_VALUE, -0.0, 0f, 0f, EntityTypes.TNT, Integer.MIN_VALUE, new Vec3(10, -20, 30), 1);
      addEntity("huge_movement", 5, uuid(5, 6), 0, 0, 0, 0f, 0f, EntityTypes.ITEM, 0, new Vec3(1.7e10, -1.7e10, 5e9), 0);
      addEntity("tiny_movement", 5, uuid(5, 6), 0, 0, 0, 0f, 0f, EntityTypes.ITEM, 0, new Vec3(3e-5, -3e-5, 3e-5), 0);
      addEntity("scale4", 6, uuid(7, 8), 0, 0, 0, 0f, 0f, EntityTypes.ITEM, 0, new Vec3(4.5, 0.0, -4.0), 0);
      addEntity("scale3", 6, uuid(7, 8), 0, 0, 0, 0f, 0f, EntityTypes.ITEM, 0, new Vec3(3.0, 2.9, -2.0), 0);
      addEntity("last_type", 8, uuid(9, 9), 0, 0, 0, 0f, 0f, BuiltInRegistries.ENTITY_TYPE.byId(BuiltInRegistries.ENTITY_TYPE.size() - 1), 0, Vec3.ZERO, 0);
      PlayHarness.raw("count_entity_types", b -> b.writeVarInt(BuiltInRegistries.ENTITY_TYPE.size()));

      // ---- set_entity_data: one value per case (accessor id 5, entity id 7)
      PlayHarness.cb("sed_empty", new ClientboundSetEntityDataPacket(0, List.of()));
      PlayHarness.cb("sed_big_entity_id", new ClientboundSetEntityDataPacket(Integer.MAX_VALUE, List.of()));
      PlayHarness.cb("sed_neg_entity_id", new ClientboundSetEntityDataPacket(-1, List.of()));
      PlayHarness.cb("sed_multi", new ClientboundSetEntityDataPacket(300, List.of(
         new SynchedEntityData.DataValue<>(0, EntityDataSerializers.BYTE, (byte) 0x20),
         new SynchedEntityData.DataValue<>(8, EntityDataSerializers.FLOAT, 20.0f),
         new SynchedEntityData.DataValue<>(254, EntityDataSerializers.BOOLEAN, true),
         new SynchedEntityData.DataValue<>(9, EntityDataSerializers.POSE, Pose.CROUCHING),
         new SynchedEntityData.DataValue<>(2, EntityDataSerializers.OPTIONAL_COMPONENT, Optional.of(Component.literal("Steve"))))));

      for (byte v : new byte[]{0, 127, -128, -1}) sed("byte_" + v, EntityDataSerializers.BYTE, v);
      for (int v : new int[]{0, 1, -1, 127, 128, 300, Integer.MAX_VALUE, Integer.MIN_VALUE}) sed("int_" + v, EntityDataSerializers.INT, v);
      for (long v : new long[]{0, 1, -1, 1L << 40, Long.MAX_VALUE, Long.MIN_VALUE}) sed("long_" + v, EntityDataSerializers.LONG, v);
      String[] fl = {"0", "neg0", "1_5", "max", "min", "inf", "ninf"};
      float[] fv = {0f, -0f, 1.5f, Float.MAX_VALUE, Float.MIN_VALUE, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY};
      for (int i = 0; i < fl.length; i++) sed("float_" + fl[i], EntityDataSerializers.FLOAT, fv[i]);
      sed("string_empty", EntityDataSerializers.STRING, "");
      sed("string_ascii", EntityDataSerializers.STRING, "Hello, world");
      sed("string_unicode", EntityDataSerializers.STRING, "h\u00e9llo \uD83D\uDE00 w\u00f6rld \u4e16\u754c");
      sed("string_max", EntityDataSerializers.STRING, "a".repeat(32767));
      sed("string_max_nonbmp", EntityDataSerializers.STRING, "\uD83D\uDE00".repeat(16383) + "x");
      Component styled = Component.literal("styled").withStyle(ChatFormatting.RED, ChatFormatting.BOLD).append(Component.literal(" tail"));
      Component translated = Component.translatable("chat.type.text", "name", Component.literal("msg").withStyle(ChatFormatting.ITALIC));
      sed("component_literal", EntityDataSerializers.COMPONENT, Component.literal("hi"));
      sed("component_empty", EntityDataSerializers.COMPONENT, Component.empty());
      sed("component_styled", EntityDataSerializers.COMPONENT, styled);
      sed("component_translatable", EntityDataSerializers.COMPONENT, translated);
      sed("component_keybind", EntityDataSerializers.COMPONENT, Component.keybind("key.jump"));
      sed("optional_component_empty", EntityDataSerializers.OPTIONAL_COMPONENT, Optional.empty());
      sed("optional_component_present", EntityDataSerializers.OPTIONAL_COMPONENT, Optional.of(styled));
      sed("boolean_true", EntityDataSerializers.BOOLEAN, true);
      sed("boolean_false", EntityDataSerializers.BOOLEAN, false);
      sed("rotations_zero", EntityDataSerializers.ROTATIONS, new Rotations(0, 0, 0));
      sed("rotations_mixed", EntityDataSerializers.ROTATIONS, new Rotations(10f, -20f, 30.5f));
      sed("rotations_wrap", EntityDataSerializers.ROTATIONS, new Rotations(400f, -721f, 360f));
      sed("rotations_nonfinite", EntityDataSerializers.ROTATIONS, new Rotations(Float.POSITIVE_INFINITY, Float.NaN, Float.NEGATIVE_INFINITY));
      sed("block_pos_origin", EntityDataSerializers.BLOCK_POS, BlockPos.ZERO);
      sed("block_pos_neg", EntityDataSerializers.BLOCK_POS, new BlockPos(-1, -1, -1));
      sed("block_pos_max", EntityDataSerializers.BLOCK_POS, new BlockPos(33554431, 2047, -33554432));
      sed("block_pos_min", EntityDataSerializers.BLOCK_POS, new BlockPos(-33554432, -2048, 33554431));
      sed("optional_block_pos_empty", EntityDataSerializers.OPTIONAL_BLOCK_POS, Optional.empty());
      sed("optional_block_pos_present", EntityDataSerializers.OPTIONAL_BLOCK_POS, Optional.of(new BlockPos(100, 64, -100)));
      for (Direction d : Direction.values()) sed("direction_" + d.getName(), EntityDataSerializers.DIRECTION, d);
      sed("optional_living_entity_reference_empty", EntityDataSerializers.OPTIONAL_LIVING_ENTITY_REFERENCE, Optional.empty());
      sed("optional_living_entity_reference_present", EntityDataSerializers.OPTIONAL_LIVING_ENTITY_REFERENCE,
         Optional.of(EntityReference.of(uuid(0x0123456789abcdefL, 0xfedcba9876543210L))));
      for (int id : new int[]{0, 1, 100, 5000, lastStateId()}) sed("block_state_" + id, EntityDataSerializers.BLOCK_STATE, stateById(id));
      sed("optional_block_state_empty", EntityDataSerializers.OPTIONAL_BLOCK_STATE, Optional.empty());
      sed("optional_block_state_air", EntityDataSerializers.OPTIONAL_BLOCK_STATE, Optional.of(Blocks.AIR.defaultBlockState()));
      for (int id : new int[]{1, 100, lastStateId()}) sed("optional_block_state_" + id, EntityDataSerializers.OPTIONAL_BLOCK_STATE, Optional.of(stateById(id)));
      sed("particle_simple", EntityDataSerializers.PARTICLE, ParticleTypes.FLAME);
      sed("particle_block", EntityDataSerializers.PARTICLE, new BlockParticleOption(ParticleTypes.BLOCK, Blocks.STONE.defaultBlockState()));
      sed("particle_dust", EntityDataSerializers.PARTICLE, new DustParticleOptions(0xFF8000, 2.5f));
      sed("particles_empty", EntityDataSerializers.PARTICLES, List.of());
      sed("particles_mixed", EntityDataSerializers.PARTICLES, List.of(
         ParticleTypes.HEART, new ShriekParticleOption(10), new BlockParticleOption(ParticleTypes.FALLING_DUST, stateById(77)), new SculkChargeParticleOptions(0.5f)));

      var villagerTypes = BuiltInRegistries.VILLAGER_TYPE;
      var professions = BuiltInRegistries.VILLAGER_PROFESSION;
      PlayHarness.raw("count_villager_type", b -> b.writeVarInt(villagerTypes.size()));
      PlayHarness.raw("count_villager_profession", b -> b.writeVarInt(professions.size()));
      int[][] vd = {{0, 0, 1}, {1, 2, 5}, {villagerTypes.size() - 1, professions.size() - 1, 3}, {0, 0, 0}, {2, 3, -3}, {1, 1, 100000}, {0, 1, Integer.MAX_VALUE}};
      for (int[] d : vd) {
         sed("villager_data_" + d[0] + "_" + d[1] + "_" + d[2], EntityDataSerializers.VILLAGER_DATA,
            new VillagerData(villagerTypes.asHolderIdMap().byIdOrThrow(d[0]), professions.asHolderIdMap().byIdOrThrow(d[1]), d[2]));
      }
      sed("optional_unsigned_int_empty", EntityDataSerializers.OPTIONAL_UNSIGNED_INT, OptionalInt.empty());
      for (int v : new int[]{0, 1, 5, 127, 300, Integer.MAX_VALUE}) sed("optional_unsigned_int_" + v, EntityDataSerializers.OPTIONAL_UNSIGNED_INT, OptionalInt.of(v));
      for (Pose p : Pose.values()) sed("pose_" + p.id(), EntityDataSerializers.POSE, p);

      variant("cat_variant", Registries.CAT_VARIANT, EntityDataSerializers.CAT_VARIANT);
      variant("cat_sound_variant", Registries.CAT_SOUND_VARIANT, EntityDataSerializers.CAT_SOUND_VARIANT);
      variant("cow_variant", Registries.COW_VARIANT, EntityDataSerializers.COW_VARIANT);
      variant("cow_sound_variant", Registries.COW_SOUND_VARIANT, EntityDataSerializers.COW_SOUND_VARIANT);
      variant("wolf_variant", Registries.WOLF_VARIANT, EntityDataSerializers.WOLF_VARIANT);
      variant("wolf_sound_variant", Registries.WOLF_SOUND_VARIANT, EntityDataSerializers.WOLF_SOUND_VARIANT);
      variant("frog_variant", Registries.FROG_VARIANT, EntityDataSerializers.FROG_VARIANT);
      variant("pig_variant", Registries.PIG_VARIANT, EntityDataSerializers.PIG_VARIANT);
      variant("pig_sound_variant", Registries.PIG_SOUND_VARIANT, EntityDataSerializers.PIG_SOUND_VARIANT);
      variant("chicken_variant", Registries.CHICKEN_VARIANT, EntityDataSerializers.CHICKEN_VARIANT);
      variant("chicken_sound_variant", Registries.CHICKEN_SOUND_VARIANT, EntityDataSerializers.CHICKEN_SOUND_VARIANT);
      variant("zombie_nautilus_variant", Registries.ZOMBIE_NAUTILUS_VARIANT, EntityDataSerializers.ZOMBIE_NAUTILUS_VARIANT);
      variant("painting_variant", Registries.PAINTING_VARIANT, EntityDataSerializers.PAINTING_VARIANT);

      sed("painting_variant_direct_a", EntityDataSerializers.PAINTING_VARIANT, Holder.direct(new net.minecraft.world.entity.decoration.painting.PaintingVariant(
         2, 3, Identifier.parse("mod:custom_art"), Optional.of(Component.literal("Title")), Optional.of(styled))));
      sed("painting_variant_direct_b", EntityDataSerializers.PAINTING_VARIANT, Holder.direct(new net.minecraft.world.entity.decoration.painting.PaintingVariant(
         -1, 300, Identifier.parse("mod:custom_art"), Optional.empty(), Optional.empty())));
      sed("optional_global_pos_empty", EntityDataSerializers.OPTIONAL_GLOBAL_POS, Optional.empty());
      sed("optional_global_pos_overworld", EntityDataSerializers.OPTIONAL_GLOBAL_POS, Optional.of(GlobalPos.of(Level.OVERWORLD, new BlockPos(1, 2, 3))));
      sed("optional_global_pos_custom", EntityDataSerializers.OPTIONAL_GLOBAL_POS,
         Optional.of(GlobalPos.of(ResourceKey.create(Registries.DIMENSION, Identifier.parse("mod:the_void")), new BlockPos(-30000000, -64, 30000000))));
      for (Sniffer.State s : Sniffer.State.values()) sed("sniffer_state_" + s.id(), EntityDataSerializers.SNIFFER_STATE, s);
      for (Armadillo.ArmadilloState s : Armadillo.ArmadilloState.values()) sed("armadillo_state_" + s.ordinal(), EntityDataSerializers.ARMADILLO_STATE, s);
      for (CopperGolemState s : CopperGolemState.values()) sed("copper_golem_state_" + s.ordinal(), EntityDataSerializers.COPPER_GOLEM_STATE, s);
      for (WeatheringCopper.WeatherState s : WeatheringCopper.WeatherState.values()) sed("weathering_copper_state_" + s.ordinal(), EntityDataSerializers.WEATHERING_COPPER_STATE, s);
      sed("vector3_zero", EntityDataSerializers.VECTOR3, new org.joml.Vector3f(0f, 0f, 0f));
      sed("vector3_mixed", EntityDataSerializers.VECTOR3, new org.joml.Vector3f(1.5f, -2.25f, 1e10f));
      sed("quaternion_identity", EntityDataSerializers.QUATERNION, new org.joml.Quaternionf(0f, 0f, 0f, 1f));
      sed("quaternion_mixed", EntityDataSerializers.QUATERNION, new org.joml.Quaternionf(0.5f, -0.5f, 0.5f, -0.5f));
      for (HumanoidArm a : HumanoidArm.values()) sed("humanoid_arm_" + a.ordinal(), EntityDataSerializers.HUMANOID_ARM, a);
      for (DyeColor c : DyeColor.values()) sed("dye_color_" + c.getId(), EntityDataSerializers.DYE_COLOR, c);

      // ---- particles (raw `ParticleTypes.STREAM_CODEC` lines, one per registry entry plus payload variants)
      for (ParticleType<?> t : BuiltInRegistries.PARTICLE_TYPE) {
         String name = BuiltInRegistries.PARTICLE_TYPE.getKey(t).getPath();
         if (t instanceof SimpleParticleType s) particle("simple_" + name, s);
      }
      BlockState stone = Blocks.STONE.defaultBlockState();
      int[] states = {0, 1, 5000, lastStateId()};
      for (ParticleType<BlockParticleOption> t : List.of(ParticleTypes.BLOCK, ParticleTypes.BLOCK_MARKER, ParticleTypes.FALLING_DUST, ParticleTypes.DUST_PILLAR, ParticleTypes.BLOCK_CRUMBLE)) {
         for (int s : states) particle("block_" + BuiltInRegistries.PARTICLE_TYPE.getKey(t).getPath() + "_" + s, new BlockParticleOption(t, stateById(s)));
      }
      for (ParticleType<ColorParticleOption> t : List.of(ParticleTypes.ENTITY_EFFECT, ParticleTypes.TINTED_LEAVES, ParticleTypes.FLASH)) {
         String n = BuiltInRegistries.PARTICLE_TYPE.getKey(t).getPath();
         for (int c : new int[]{0, -1, 0xFFFF0000, 0x12345678, Integer.MIN_VALUE}) particle("color_" + n + "_" + c, ColorParticleOption.create(t, c));
      }
      float[] powers = {0f, 1f, -2.5f, 1e9f};
      for (int i = 0; i < powers.length; i++) particle("power_" + i, PowerParticleOption.create(ParticleTypes.DRAGON_BREATH, powers[i]));
      for (ParticleType<SpellParticleOption> t : List.of(ParticleTypes.EFFECT, ParticleTypes.INSTANT_EFFECT)) {
         String n = BuiltInRegistries.PARTICLE_TYPE.getKey(t).getPath();
         particle("spell_" + n + "_a", SpellParticleOption.create(t, 0xFFFFFF, 1.0f));
         particle("spell_" + n + "_b", SpellParticleOption.create(t, -1, 0.25f));
         particle("spell_" + n + "_c", SpellParticleOption.create(t, 0x00FF7F, -3f));
      }
      for (ParticleType<GeyserParticleOptions> t : List.of(ParticleTypes.GEYSER, ParticleTypes.GEYSER_PLUME)) {
         String n = BuiltInRegistries.PARTICLE_TYPE.getKey(t).getPath();
         for (int w : new int[]{0, 5, -3, Integer.MAX_VALUE}) particle("geyser_" + n + "_" + w, new GeyserParticleOptions(t, w));
      }
      for (ParticleType<GeyserBaseParticleOptions> t : List.of(ParticleTypes.GEYSER_BASE, ParticleTypes.GEYSER_POOF)) {
         String n = BuiltInRegistries.PARTICLE_TYPE.getKey(t).getPath();
         particle("geyser_base_" + n + "_a", new GeyserBaseParticleOptions(t, 3, 1.5f));
         particle("geyser_base_" + n + "_b", new GeyserBaseParticleOptions(t, -1, -0.5f));
      }
      particle("dust_a", new DustParticleOptions(0xFF0000, 1.0f));
      particle("dust_b", new DustParticleOptions(0x123456, 0.01f));
      particle("dust_c", new DustParticleOptions(-1, 4.0f));
      particle("dust_clamp_hi", new DustParticleOptions(0, 100f));
      particle("dust_clamp_lo", new DustParticleOptions(0, -3f));
      particle("dust_color_transition_a", new DustColorTransitionOptions(0x3A0AE0, 0xFF0000, 1.0f));
      particle("dust_color_transition_b", new DustColorTransitionOptions(0, -1, 0.5f));
      particle("dust_color_transition_clamp_hi", new DustColorTransitionOptions(1, 2, 50f));
      particle("dust_color_transition_clamp_lo", new DustColorTransitionOptions(1, 2, 0f));
      float[] rolls = {0f, -1.5f, 3.14159f};
      for (int i = 0; i < rolls.length; i++) particle("sculk_charge_" + i, new SculkChargeParticleOptions(rolls[i]));
      for (int d : new int[]{0, 1, 100, 300, -1, Integer.MAX_VALUE}) particle("shriek_" + d, new ShriekParticleOption(d));
      RegistryFriendlyByteBuf ps = buf();
      ps.writeVarInt(1); ps.writeVarInt(77777); ps.writeFloat(1.5f);
      PositionSource entitySource = PositionSource.STREAM_CODEC.decode(ps);
      particle("vibration_block", new VibrationParticleOption(new net.minecraft.world.level.gameevent.BlockPositionSource(new BlockPos(1, -2, 3)), 20));
      particle("vibration_block_extreme", new VibrationParticleOption(new net.minecraft.world.level.gameevent.BlockPositionSource(new BlockPos(-33554432, 2047, 33554431)), 0));
      particle("vibration_entity", new VibrationParticleOption(entitySource, 300));
      particle("vibration_negative_ticks", new VibrationParticleOption(new net.minecraft.world.level.gameevent.BlockPositionSource(BlockPos.ZERO), -5));
      particle("trail_a", new TrailParticleOption(new Vec3(1.5, 64.0, -3.25), 0xFF00FF, 40));
      particle("trail_b", new TrailParticleOption(new Vec3(-1e9, 0.0, 1e-9), -1, Integer.MAX_VALUE));

      // ---- item dependent: ITEM_STACK serializer, RESOLVABLE_PROFILE serializer, `item` particle
      ItemStack named = new ItemStack(Items.DIAMOND_SWORD);
      named.set(DataComponents.UNBREAKABLE, net.minecraft.util.Unit.INSTANCE);
      named.set(DataComponents.DAMAGE, 12);
      named.set(DataComponents.MAX_STACK_SIZE, 16);
      ItemStack stripped = new ItemStack(Items.DIAMOND_PICKAXE, 3);
      stripped.remove(DataComponents.MAX_DAMAGE);
      sed("item_stack_empty", EntityDataSerializers.ITEM_STACK, ItemStack.EMPTY);
      sed("item_stack_diamond", EntityDataSerializers.ITEM_STACK, new ItemStack(Items.DIAMOND));
      sed("item_stack_stone_64", EntityDataSerializers.ITEM_STACK, new ItemStack(Items.STONE, 64));
      sed("item_stack_components", EntityDataSerializers.ITEM_STACK, named);
      sed("item_stack_removed_component", EntityDataSerializers.ITEM_STACK, stripped);
      PlayHarness.raw("count_items", b -> b.writeVarInt(BuiltInRegistries.ITEM.size()));
      sed("item_stack_last_item", EntityDataSerializers.ITEM_STACK, new ItemStack(BuiltInRegistries.ITEM.byId(BuiltInRegistries.ITEM.size() - 1)));
      GameProfile full = new GameProfile(uuid(0x0123456789abcdefL, 0xfedcba9876543210L), "Alex", new PropertyMap(
         com.google.common.collect.ImmutableMultimap.of("textures", new Property("textures", "base64value", "sig"), "other", new Property("other", "v"))));
      sed("resolvable_profile_name", EntityDataSerializers.RESOLVABLE_PROFILE, ResolvableProfile.createUnresolved("Steve"));
      sed("resolvable_profile_id", EntityDataSerializers.RESOLVABLE_PROFILE, ResolvableProfile.createUnresolved(uuid(1, 2)));
      sed("resolvable_profile_full", EntityDataSerializers.RESOLVABLE_PROFILE, ResolvableProfile.createResolved(full));
      sed("resolvable_profile_full_no_props", EntityDataSerializers.RESOLVABLE_PROFILE, ResolvableProfile.createResolved(new GameProfile(uuid(3, 4), "Bob")));
      sed("resolvable_profile_static_empty", EntityDataSerializers.RESOLVABLE_PROFILE, ResolvableProfile.Static.EMPTY);
      particle("item_diamond", new ItemParticleOption(ParticleTypes.ITEM, Items.DIAMOND));
      particle("item_count_components", new ItemParticleOption(ParticleTypes.ITEM, new ItemStackTemplate(Items.STONE.builtInRegistryHolder(), 5,
         DataComponentPatch.builder().set(DataComponents.DAMAGE, 3).remove(DataComponents.MAX_STACK_SIZE).build())));
      sed("particle_item", EntityDataSerializers.PARTICLE, new ItemParticleOption(ParticleTypes.ITEM, Items.APPLE));
      sed("particles_item", EntityDataSerializers.PARTICLES, List.of(new ItemParticleOption(ParticleTypes.ITEM, Items.APPLE), ParticleTypes.FLAME));
      PlayHarness.cb("level_particles_item", new ClientboundLevelParticlesPacket(new ItemParticleOption(ParticleTypes.ITEM, Items.EMERALD), false, true, 1, 2, 3, 0f, 0f, 0f, 0f, 1));
      redecode("redecode_sed_item_stack_zero_count", sedBytes(7, b -> b.writeVarInt(0)));
      redecode("redecode_sed_item_stack_bad_item", sedBytes(7, b -> { b.writeVarInt(1); b.writeVarInt(BuiltInRegistries.ITEM.size()); b.writeVarInt(0); b.writeVarInt(0); }));
      redecode("redecode_sed_item_stack_negative_count", sedBytes(7, b -> { b.writeVarInt(-1); }));
      redecode("redecode_sed_item_particle_bad_item", sedBytes(16, b -> { b.writeVarInt(ParticleTypeId.ITEM); b.writeVarInt(BuiltInRegistries.ITEM.size()); b.writeVarInt(1); b.writeVarInt(0); b.writeVarInt(0); }));
      redecode("redecode_sed_item_particle_zero_count", sedBytes(16, b -> { b.writeVarInt(ParticleTypeId.ITEM); b.writeVarInt(1); b.writeVarInt(0); b.writeVarInt(0); b.writeVarInt(0); }));
      redecode("redecode_sed_profile_name_too_long", sedBytes(41, b -> { b.writeBoolean(false); b.writeBoolean(true); b.writeUtf("a".repeat(17), 17); b.writeBoolean(false); b.writeVarInt(0); for (int i = 0; i < 4; i++) b.writeBoolean(false); }));
      redecode("redecode_sed_profile_too_many_props", sedBytes(41, b -> { b.writeBoolean(false); b.writeBoolean(false); b.writeBoolean(false); b.writeVarInt(17); }));
      redecode("redecode_sed_profile_skin_patch", sedBytes(41, b -> { b.writeBoolean(false); b.writeBoolean(false); b.writeBoolean(false); b.writeVarInt(0);
         b.writeBoolean(true); b.writeUtf("mod:t"); b.writeBoolean(false); b.writeBoolean(true); b.writeUtf("mod:e"); b.writeBoolean(true); b.writeBoolean(true); }));
      redecode("redecode_sed_profile_skin_patch_wide", sedBytes(41, b -> { b.writeBoolean(true); b.writeUUID(uuid(1, 1)); b.writeUtf("N", 16); b.writeVarInt(0);
         for (int i = 0; i < 3; i++) b.writeBoolean(false); b.writeBoolean(true); b.writeBoolean(false); }));
      redecode("redecode_sed_profile_property_name_too_long", sedBytes(41, b -> { b.writeBoolean(false); b.writeBoolean(false); b.writeBoolean(false); b.writeVarInt(1); b.writeUtf("n".repeat(65), 100); }));
      redecode("redecode_sed_profile_partial_both", sedBytes(41, b -> { b.writeBoolean(false); b.writeBoolean(true); b.writeUtf("Zed", 16); b.writeBoolean(true); b.writeUUID(uuid(5, 6)); b.writeVarInt(0); for (int i = 0; i < 4; i++) b.writeBoolean(false); }));

      // ---- level_particles
      ParticleOptions[] pk = {ParticleTypes.FLAME, new DustParticleOptions(0xFF0000, 1.0f), new BlockParticleOption(ParticleTypes.BLOCK, stone),
         new VibrationParticleOption(new net.minecraft.world.level.gameevent.BlockPositionSource(new BlockPos(4, 5, 6)), 10), ParticleTypes.EXPLOSION};
      int n = 0;
      for (ParticleOptions p : pk) {
         PlayHarness.cb("level_particles_" + n++, new ClientboundLevelParticlesPacket(p, n % 2 == 0, n % 3 == 0, 1.5, -64.25, 1e6, 0.1f, 0.2f, 0.3f, 0.01f, 0.02f, 0.03f, n * 7, ClientboundLevelParticlesPacket.RandomizationType.values()[n % 3]));
      }
      PlayHarness.cb("level_particles_simple_ctor", new ClientboundLevelParticlesPacket(ParticleTypes.HAPPY_VILLAGER, true, true, 0, 0, 0, 0f, 0f, 0f, 0.5f, 0));
      PlayHarness.cb("level_particles_extreme", new ClientboundLevelParticlesPacket(ParticleTypes.CRIT, false, false, Double.MAX_VALUE, -0.0, Double.MIN_VALUE,
         Float.MAX_VALUE, -Float.MAX_VALUE, Float.MIN_VALUE, -1f, 1f, 1e-9f, Integer.MAX_VALUE, ClientboundLevelParticlesPacket.RandomizationType.ALTERNATIVE_WITH_SPEED));
      PlayHarness.cb("level_particles_neg_count", new ClientboundLevelParticlesPacket(ParticleTypes.CRIT, false, true, 0, 0, 0, 0f, 0f, 0f, 0f, 0f, 0f, -1, ClientboundLevelParticlesPacket.RandomizationType.DEFAULT));
      for (var r : ClientboundLevelParticlesPacket.RandomizationType.values())
         PlayHarness.cb("level_particles_rand_" + r.ordinal(), new ClientboundLevelParticlesPacket(ParticleTypes.CRIT, false, true, 0, 0, 0, 0f, 0f, 0f, 0f, 0f, 0f, 3, r));

      // ---- explode
      Holder<SoundEvent> directVar = Holder.direct(SoundEvent.createVariableRangeEvent(Identifier.parse("mod:boom")));
      Holder<SoundEvent> directFixed = Holder.direct(SoundEvent.createFixedRangeEvent(Identifier.withDefaultNamespace("entity.generic.explode"), 24.5f));
      WeightedList<ExplosionParticleInfo> none = WeightedList.of();
      WeightedList<ExplosionParticleInfo> some = WeightedList.of(List.of(
         new Weighted<>(new ExplosionParticleInfo(ParticleTypes.POOF, 1.0f, 2.0f), 3),
         new Weighted<>(new ExplosionParticleInfo(new DustParticleOptions(0x808080, 1.5f), 0.5f, 0.25f), 1),
         new Weighted<>(new ExplosionParticleInfo(new BlockParticleOption(ParticleTypes.BLOCK, stone), -1f, 0f), 0)));
      PlayHarness.cb("explode_basic", new ClientboundExplodePacket(new Vec3(1.5, 64.0, -3.25), 4.0f, 10, Optional.empty(), ParticleTypes.EXPLOSION, SoundEvents.GENERIC_EXPLODE, none, true));
      PlayHarness.cb("explode_knockback", new ClientboundExplodePacket(new Vec3(-1e9, 0, 1e9), 0f, 0, Optional.of(new Vec3(0.5, -0.25, 1.0)), ParticleTypes.EXPLOSION_EMITTER, SoundEvents.GENERIC_EXPLODE, some, false));
      PlayHarness.cb("explode_direct_var", new ClientboundExplodePacket(Vec3.ZERO, 1f, -1, Optional.of(Vec3.ZERO), new DustParticleOptions(0xFFFFFF, 1f), directVar, some, true));
      PlayHarness.cb("explode_direct_fixed", new ClientboundExplodePacket(Vec3.ZERO, Float.MAX_VALUE, Integer.MAX_VALUE, Optional.empty(), ParticleTypes.SMOKE, directFixed, none, false));
      PlayHarness.cb("explode_many_particles", new ClientboundExplodePacket(Vec3.ZERO, 3f, 100, Optional.empty(), ParticleTypes.EXPLOSION, SoundEvents.GENERIC_EXPLODE,
         WeightedList.of(java.util.stream.IntStream.range(0, 200).mapToObj(i -> new Weighted<>(new ExplosionParticleInfo(ParticleTypes.POOF, i, -i), i)).toList()), true));
      int soundCount = BuiltInRegistries.SOUND_EVENT.size();
      PlayHarness.raw("count_sound_events", b -> b.writeVarInt(soundCount));
      Holder<SoundEvent> lastSound = BuiltInRegistries.SOUND_EVENT.asHolderIdMap().byIdOrThrow(soundCount - 1);
      PlayHarness.cb("explode_last_sound", new ClientboundExplodePacket(Vec3.ZERO, 1f, 1, Optional.empty(), ParticleTypes.EXPLOSION, lastSound, none, true));

      // ---- vanilla's reaction to hostile input: decode with the real codec, re-encode (or the error text)
      redecode("redecode_sed_oob_pose", sedBytes(20, b -> b.writeVarInt(99)));
      redecode("redecode_sed_neg_pose", sedBytes(20, b -> b.writeVarInt(-4)));
      redecode("redecode_sed_oob_direction", sedBytes(12, b -> b.writeVarInt(1000)));
      redecode("redecode_sed_oob_sniffer", sedBytes(35, b -> b.writeVarInt(9)));
      redecode("redecode_sed_oob_armadillo", sedBytes(36, b -> b.writeVarInt(-1)));
      redecode("redecode_sed_oob_copper_golem", sedBytes(37, b -> b.writeVarInt(77)));
      redecode("redecode_sed_oob_weathering_hi", sedBytes(38, b -> b.writeVarInt(77)));
      redecode("redecode_sed_oob_weathering_lo", sedBytes(38, b -> b.writeVarInt(-77)));
      redecode("redecode_sed_oob_arm", sedBytes(42, b -> b.writeVarInt(5)));
      redecode("redecode_sed_oob_dye", sedBytes(43, b -> b.writeVarInt(16)));
      redecode("redecode_sed_oob_block_state", sedBytes(14, b -> b.writeVarInt(Block.BLOCK_STATE_REGISTRY.size())));
      redecode("redecode_sed_oob_optional_block_state", sedBytes(15, b -> b.writeVarInt(Block.BLOCK_STATE_REGISTRY.size() + 5)));
      redecode("redecode_sed_neg_optional_block_state", sedBytes(15, b -> b.writeVarInt(-1)));
      redecode("redecode_sed_villager_level_zero", sedBytes(18, b -> { b.writeVarInt(1); b.writeVarInt(1); b.writeVarInt(-9); }));
      redecode("redecode_sed_villager_bad_type", sedBytes(18, b -> { b.writeVarInt(999); b.writeVarInt(1); b.writeVarInt(1); }));
      redecode("redecode_sed_villager_bad_profession", sedBytes(18, b -> { b.writeVarInt(1); b.writeVarInt(999); b.writeVarInt(1); }));
      redecode("redecode_sed_rotations_unnormalised", sedBytes(9, b -> { b.writeFloat(720.5f); b.writeFloat(Float.NaN); b.writeFloat(-400f); }));
      redecode("redecode_sed_optional_int_neg", sedBytes(19, b -> b.writeVarInt(-1)));
      redecode("redecode_sed_bad_serializer", sedBytes(44, b -> {}));
      redecode("redecode_sed_bad_serializer_neg", sedBytes(-1, b -> {}));
      redecode("redecode_sed_bad_serializer_big", sedBytes(100000, b -> {}));
      redecode("redecode_sed_string_too_long", sedBytes(4, b -> b.writeUtf("a".repeat(32767) + "b", 40000)));
      redecode("redecode_sed_truncated", sedBytes(3, b -> b.writeByte(0)));
      redecode("redecode_sed_unterminated", noTerminator());
      redecode("redecode_sed_bad_particle", sedBytes(16, b -> b.writeVarInt(5000)));
      redecode("redecode_sed_particle_bad_block", sedBytes(16, b -> { b.writeVarInt(1); b.writeVarInt(Block.BLOCK_STATE_REGISTRY.size() + 1); }));
      redecode("redecode_sed_particles_neg_count", sedBytes(17, b -> b.writeVarInt(-1)));
      redecode("redecode_sed_particle_bad_position_source", sedBytes(16, b -> { b.writeVarInt(ParticleTypeId.VIBRATION); b.writeVarInt(2); }));
      redecode("redecode_sed_particle_shriek_trail", sedBytes(17, b -> {
         b.writeVarInt(2);
         b.writeVarInt(ParticleTypeId.SHRIEK); b.writeVarInt(-1);
         b.writeVarInt(ParticleTypeId.TRAIL); b.writeDouble(1); b.writeDouble(2); b.writeDouble(3); b.writeInt(4); b.writeVarInt(5);
      }));
      redecode("redecode_add_entity_bad_type", rawPacket(ClientboundAddEntityPacket.class, b -> {
         b.writeVarInt(1); b.writeUUID(uuid(1, 2)); b.writeVarInt(BuiltInRegistries.ENTITY_TYPE.size());
         addEntityTail(b);
      }));
      redecode("redecode_add_entity_neg_type", rawPacket(ClientboundAddEntityPacket.class, b -> {
         b.writeVarInt(1); b.writeUUID(uuid(1, 2)); b.writeVarInt(-1);
         addEntityTail(b);
      }));
      redecode("redecode_particles_bad_type", rawPacket(ClientboundLevelParticlesPacket.class, b -> b.writeVarInt(BuiltInRegistries.PARTICLE_TYPE.size())));
      redecode("redecode_particles_rand_oob", rawPacket(ClientboundLevelParticlesPacket.class, b -> {
         b.writeVarInt(ParticleTypeId.FLAME); b.writeBoolean(false); b.writeBoolean(false);
         b.writeDouble(0); b.writeDouble(0); b.writeDouble(0);
         for (int i = 0; i < 6; i++) b.writeFloat(0f);
         b.writeVarInt(1); b.writeVarInt(9);
      }));
      redecode("redecode_explode_neg_weight", explodeWithWeights(new int[]{-1}));
      redecode("redecode_explode_weight_overflow", explodeWithWeights(new int[]{Integer.MAX_VALUE, 1}));
      redecode("redecode_explode_weight_max", explodeWithWeights(new int[]{Integer.MAX_VALUE}));
      redecode("redecode_explode_weights_zero", explodeWithWeights(new int[]{0, 0}));
      redecode("redecode_explode_bad_sound", rawPacket(ClientboundExplodePacket.class, b -> {
         for (int i = 0; i < 3; i++) b.writeDouble(0);
         b.writeFloat(1f); b.writeInt(0); b.writeBoolean(false);
         b.writeVarInt(ParticleTypeId.EXPLOSION);
         b.writeVarInt(soundCount + 1);
      }));
      redecode("redecode_explode_neg_list", rawPacket(ClientboundExplodePacket.class, b -> {
         for (int i = 0; i < 3; i++) b.writeDouble(0);
         b.writeFloat(1f); b.writeInt(0); b.writeBoolean(false);
         b.writeVarInt(ParticleTypeId.EXPLOSION);
         b.writeVarInt(1);
         b.writeVarInt(-1);
      }));
      PlayHarness.close();
   }

   static void addEntityTail(RegistryFriendlyByteBuf b) {
      b.writeDouble(1); b.writeDouble(2); b.writeDouble(3);
      b.writeByte(0); // zero movement
      b.writeByte(1); b.writeByte(2); b.writeByte(3);
      b.writeVarInt(9);
   }

   static final class ParticleTypeId {
      static final int FLAME = id(ParticleTypes.FLAME), VIBRATION = id(ParticleTypes.VIBRATION), SHRIEK = id(ParticleTypes.SHRIEK),
         TRAIL = id(ParticleTypes.TRAIL), ITEM = id(ParticleTypes.ITEM), EXPLOSION = id(ParticleTypes.EXPLOSION);
      static int id(ParticleType<?> t) { return BuiltInRegistries.PARTICLE_TYPE.getId(t); }
   }

   static int packetId(Class<?> c) {
      return switch (c.getSimpleName()) {
         case "ClientboundAddEntityPacket" -> ids("add_entity");
         case "ClientboundSetEntityDataPacket" -> ids("set_entity_data");
         case "ClientboundLevelParticlesPacket" -> ids("level_particles");
         case "ClientboundExplodePacket" -> ids("explode");
         default -> throw new IllegalArgumentException(c.getName());
      };
   }

   static int ids(String name) {
      // Encode a throw-away packet with the real codec and read its VarInt id back.
      Packet<?> p = switch (name) {
         case "add_entity" -> new ClientboundAddEntityPacket(0, uuid(0, 0), 0, 0, 0, 0f, 0f, EntityTypes.PIG, 0, Vec3.ZERO, 0);
         case "set_entity_data" -> new ClientboundSetEntityDataPacket(0, List.of());
         case "level_particles" -> new ClientboundLevelParticlesPacket(ParticleTypes.FLAME, false, false, 0, 0, 0, 0f, 0f, 0f, 0f, 0);
         default -> new ClientboundExplodePacket(Vec3.ZERO, 0f, 0, Optional.empty(), ParticleTypes.EXPLOSION, SoundEvents.GENERIC_EXPLODE, WeightedList.of(), false);
      };
      ByteBuf out = Unpooled.buffer();
      @SuppressWarnings({"rawtypes", "unchecked"})
      var codec = (net.minecraft.network.codec.StreamCodec) PlayHarness.clientbound.codec();
      codec.encode(out, p);
      return net.minecraft.network.VarInt.read(out);
   }

   static ByteBuf rawPacket(Class<?> c, java.util.function.Consumer<RegistryFriendlyByteBuf> body) {
      RegistryFriendlyByteBuf b = buf();
      b.writeVarInt(packetId(c));
      body.accept(b);
      return b;
   }

   /** set_entity_data with entity id 7 and one value (accessor id 5, given serializer id, body written by `body`) + terminator. */
   static ByteBuf sedBytes(int serializerId, java.util.function.Consumer<RegistryFriendlyByteBuf> body) {
      return rawPacket(ClientboundSetEntityDataPacket.class, b -> {
         b.writeVarInt(7);
         b.writeByte(5);
         b.writeVarInt(serializerId);
         body.accept(b);
         b.writeByte(255);
      });
   }

   static ByteBuf noTerminator() {
      return rawPacket(ClientboundSetEntityDataPacket.class, b -> { b.writeVarInt(7); b.writeByte(5); b.writeVarInt(0); b.writeByte(1); });
   }

   static ByteBuf explodeWithWeights(int[] weights) {
      return rawPacket(ClientboundExplodePacket.class, b -> {
         for (int i = 0; i < 3; i++) b.writeDouble(0);
         b.writeFloat(1f); b.writeInt(0); b.writeBoolean(false);
         b.writeVarInt(ParticleTypeId.EXPLOSION);
         b.writeVarInt(1);
         b.writeVarInt(weights.length);
         for (int w : weights) { b.writeVarInt(ParticleTypeId.EXPLOSION); b.writeFloat(1f); b.writeFloat(1f); b.writeVarInt(w); }
         b.writeBoolean(true);
      });
   }
}
