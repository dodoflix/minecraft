// Golden generator for the play `entity` packet group (entity.golden): see PlayHarness.java / gen.sh.
//   crates/mc-protocol/tests/fixtures/play/gen.sh GenEntity.java
import com.google.common.collect.ImmutableList;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import com.mojang.datafixers.util.Pair;
import java.util.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.util.Unit;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.commands.arguments.EntityAnchorArgument.Anchor;
import net.minecraft.core.Holder;
import net.minecraft.core.PositionAndRotation;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.damagesource.DamageType;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.PositionMoveRotation;
import net.minecraft.world.entity.PositionPath;
import net.minecraft.world.entity.PositionStep;
import net.minecraft.world.entity.Relative;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.entity.player.Abilities;
import net.minecraft.world.entity.vehicle.minecart.NewMinecartBehavior.MinecartStep;
import net.minecraft.world.item.SwingAnimationType;
import net.minecraft.world.item.component.SwingAnimation;
import net.minecraft.world.phys.Vec3;

public class GenEntity {
   static sun.misc.Unsafe unsafe;

   /** A bare entity with only its id set (all these packets read nothing else; id 0 is rejected by `getId`). */
   static Entity ent(int id) throws Exception {
      Entity e = (Entity)unsafe.allocateInstance(ArmorStand.class);
      e.setId(id);
      return e;
   }

   /** A packet instance with its fields set reflectively (for states the public constructors cannot produce). */
   static <T> T alloc(Class<T> c, Object... nameValue) throws Exception {
      T t = c.cast(unsafe.allocateInstance(c));
      for (int i = 0; i < nameValue.length; i += 2) {
         Field f = c.getDeclaredField((String)nameValue[i]);
         f.setAccessible(true);
         f.set(t, nameValue[i + 1]);
      }
      return t;
   }

   static final int[] IDS = {1, 2, 127, 128, 255, 16383, 16384, 2097151, 2097152, 268435455, 268435456, Integer.MAX_VALUE, -1, -128, Integer.MIN_VALUE};
   static final double[] DOUBLES = {0.0, -0.0, 1.0, -1.5, 0.1, 1e-300, 1e300, Double.MAX_VALUE, Double.MIN_VALUE, Double.NEGATIVE_INFINITY, 123456.789012345};
   static final float[] FLOATS = {0.0f, -0.0f, 1.0f, -90.5f, 180.0f, 360.25f, 0.1f, Float.MAX_VALUE, Float.MIN_VALUE, Float.POSITIVE_INFINITY};

   static Vec3 v(int i) {
      return new Vec3(DOUBLES[i % DOUBLES.length], DOUBLES[(i * 3 + 1) % DOUBLES.length], DOUBLES[(i * 7 + 2) % DOUBLES.length]);
   }

   public static void main(String[] args) throws Exception {
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      uf.setAccessible(true);
      unsafe = (sun.misc.Unsafe)uf.get(null);
      PlayHarness.init(args[0]);

      // animate / hurt animation / entity event / rotate head / camera / take item / mount screen / projectile power
      for (int i = 0; i < IDS.length; i++) {
         PlayHarness.cb("animate/" + i, new ClientboundAnimatePacket(ent(IDS[i]), i % 3 == 0 ? 0 : i % 3 == 1 ? 1 : 2));
         PlayHarness.cb("hurt_animation/" + i, new ClientboundHurtAnimationPacket(IDS[i], FLOATS[i % FLOATS.length]));
         PlayHarness.cb("entity_event/" + i, new ClientboundEntityEventPacket(ent(IDS[i]), (byte)(i * 17 - 128)));
         PlayHarness.cb("rotate_head/" + i, new ClientboundRotateHeadPacket(ent(IDS[i]), (byte)(i * 19 - 128)));
         PlayHarness.cb("set_camera/" + i, new ClientboundSetCameraPacket(ent(IDS[i])));
         PlayHarness.cb("take_item_entity/" + i, new ClientboundTakeItemEntityPacket(IDS[i], IDS[(i + 4) % IDS.length], IDS[(i + 9) % IDS.length]));
         PlayHarness.cb("mount_screen_open/" + i, new ClientboundMountScreenOpenPacket(IDS[i], IDS[(i + 3) % IDS.length], IDS[(i + 6) % IDS.length]));
         PlayHarness.cb("projectile_power/" + i, new ClientboundProjectilePowerPacket(IDS[i], DOUBLES[i % DOUBLES.length]));
      }
      PlayHarness.cb("animate/id0", alloc(ClientboundAnimatePacket.class, "id", 0, "action", 255));
      PlayHarness.cb("animate/action255", alloc(ClientboundAnimatePacket.class, "id", 5, "action", 255));
      PlayHarness.cb("entity_event/id0", alloc(ClientboundEntityEventPacket.class, "entityId", 0, "eventId", (byte)127));
      PlayHarness.cb("rotate_head/id0", alloc(ClientboundRotateHeadPacket.class, "entityId", 0, "yHeadRot", (byte)-1));
      PlayHarness.cb("set_camera/id0", alloc(ClientboundSetCameraPacket.class, "cameraId", 0));

      // swing animation: every hand x animation type
      int sw = 0;
      for (InteractionHand hand : InteractionHand.values()) {
         for (SwingAnimationType t : SwingAnimationType.values()) {
            for (int d : new int[]{0, 6, 300, -1}) {
               PlayHarness.cb("swing_animation/" + sw++, new ClientboundSwingAnimationPacket(1000 + sw, hand, new SwingAnimation(t, d)));
            }
         }
      }
      PlayHarness.cb("swing_animation/id0", new ClientboundSwingAnimationPacket(0, InteractionHand.OFF_HAND, new SwingAnimation(SwingAnimationType.STAB, Integer.MAX_VALUE)));
      for (SwingAnimationType t : SwingAnimationType.values()) {
         PlayHarness.raw("raw/swing_animation_type/" + t.ordinal(), b -> SwingAnimationType.STREAM_CODEC.encode(b, t));
      }

      // damage event
      Registry<DamageType> damageTypes = PlayHarness.access.lookupOrThrow(Registries.DAMAGE_TYPE);
      int n = 0;
      for (int typeId : new int[]{0, 1, 15, 31}) {
         Holder<DamageType> h = damageTypes.asHolderIdMap().byIdOrThrow(typeId);
         for (int[] ids : new int[][]{{1, -1, -1}, {300, 5, -1}, {-5, -1, 7}, {Integer.MAX_VALUE, 0, 0}, {16384, 16383, 16385}}) {
            PlayHarness.cb("damage_event/" + n++, new ClientboundDamageEventPacket(ids[0], h, ids[1], ids[2], Optional.empty()));
            PlayHarness.cb("damage_event/" + n++, new ClientboundDamageEventPacket(ids[0], h, ids[1], ids[2], Optional.of(v(n))));
         }
      }

      // move entity: pos / pos+rot / rot, linear + stepped deltas, on-ground both ways
      short[][] deltas = {{0, 0, 0}, {1, -1, 2}, {32767, -32768, 255}, {-300, 4096, -4096}, {128, 0, -129}};
      n = 0;
      for (boolean ground : new boolean[]{false, true}) {
         for (short[] d : deltas) {
            VecDelta lin = new VecDelta.Linear(d[0], d[1], d[2]);
            PlayHarness.cb("move_entity_pos/" + n, new ClientboundMoveEntityPacket.Pos(IDS[n % IDS.length], lin, ground));
            PlayHarness.cb("move_entity_pos_rot/" + n, new ClientboundMoveEntityPacket.PosRot(IDS[n % IDS.length], lin, (byte)(n * 31), (byte)(-n * 29), ground));
            n++;
         }
      }
      for (int steps : new int[]{1, 2, 3, 10, 63, 64, 200}) {
         List<VecDelta.Stepped.DeltaStep> list = new ArrayList<>();
         for (int i = 0; i < steps; i++) {
            list.add(new VecDelta.Stepped.DeltaStep((short)(i * 7 - 100), (short)(-i * 11), (short)(i * 300), i * i * 5));
         }
         VecDelta st = new VecDelta.Stepped(list);
         PlayHarness.cb("move_entity_pos/stepped" + steps, new ClientboundMoveEntityPacket.Pos(42, st, steps % 2 == 0));
         PlayHarness.cb("move_entity_pos_rot/stepped" + steps, new ClientboundMoveEntityPacket.PosRot(-42, st, (byte)-128, (byte)127, steps % 2 == 1));
      }
      PlayHarness.cb("move_entity_pos/stepped_empty", new ClientboundMoveEntityPacket.Pos(9, new VecDelta.Stepped(List.of()), true));
      n = 0;
      for (boolean ground : new boolean[]{false, true}) {
         for (int i = 0; i < IDS.length; i++) {
            PlayHarness.cb("move_entity_rot/" + n++, new ClientboundMoveEntityPacket.Rot(IDS[i], (byte)(i * 23 - 128), (byte)(127 - i * 17), ground));
         }
      }

      // minecart steps
      PlayHarness.cb("move_minecart/empty", new ClientboundMoveMinecartPacket(7, List.of()));
      PlayHarness.cb("move_minecart/zero", new ClientboundMoveMinecartPacket(Integer.MAX_VALUE, List.of(MinecartStep.ZERO)));
      for (int count : new int[]{1, 2, 5, 130}) {
         List<MinecartStep> steps = new ArrayList<>();
         for (int i = 0; i < count; i++) {
            steps.add(new MinecartStep(v(i), v(i + 5), (float)(i * 11 - 90), (float)(i * 3 % 180), FLOATS[i % FLOATS.length]));
         }
         PlayHarness.cb("move_minecart/" + count, new ClientboundMoveMinecartPacket(count * 1000, steps));
      }
      PlayHarness.raw("raw/minecart_step", b -> MinecartStep.STREAM_CODEC.encode(b, new MinecartStep(v(3), v(4), 45.0f, -45.0f, 0.5f)));

      // position sync + position path
      n = 0;
      for (boolean ground : new boolean[]{false, true}) {
         for (int i = 0; i < 4; i++) {
            PlayHarness.cb("entity_position_sync/linear" + n++, new ClientboundEntityPositionSyncPacket(IDS[i * 3], PositionPath.of(v(i)), FLOATS[i], FLOATS[i + 4], ground));
         }
      }
      for (int count : new int[]{1, 2, 3, 40}) {
         List<PositionStep> steps = new ArrayList<>();
         for (int i = 0; i < count; i++) {
            steps.add(new PositionStep(v(i + 2), i * i * 100 - 3));
         }
         PlayHarness.cb("entity_position_sync/stepped" + count, new ClientboundEntityPositionSyncPacket(count, PositionPath.stepped(steps), 12.5f, -3.5f, count % 2 == 0));
         PlayHarness.raw("raw/position_path/stepped" + count, b -> PositionPath.STREAM_CODEC.encode(b, PositionPath.stepped(steps)));
      }
      PlayHarness.raw("raw/position_path/linear", b -> PositionPath.STREAM_CODEC.encode(b, PositionPath.of(v(1))));

      // teleport / player position: every single relative, none, all, rotation/delta groups
      List<Set<Relative>> sets = new ArrayList<>();
      sets.add(Set.of());
      for (Relative r : Relative.values()) {
         sets.add(Set.of(r));
      }
      sets.add(Relative.ALL);
      sets.add(Relative.ROTATION);
      sets.add(Relative.DELTA);
      sets.add(EnumSet.of(Relative.X, Relative.DELTA_Z, Relative.Y_ROT));
      n = 0;
      for (Set<Relative> rel : sets) {
         PositionMoveRotation pmr = new PositionMoveRotation(v(n), v(n + 3), FLOATS[n % FLOATS.length], FLOATS[(n + 2) % FLOATS.length]);
         PlayHarness.cb("teleport_entity/" + n, ClientboundTeleportEntityPacket.teleport(IDS[n % IDS.length], pmr, rel, n % 2 == 0));
         PlayHarness.cb("player_position/" + n, ClientboundPlayerPositionPacket.of(IDS[(n + 1) % IDS.length], pmr, rel));
         n++;
      }

      // set entity motion (low-precision vec3, all scale classes)
      double[][] lp = {{0, 0, 0}, {1e-6, 0, 0}, {0.5, -0.25, 0.125}, {1, 1, 1}, {-1, 2, -3}, {3, 3, 3}, {4, 0, 0}, {100.5, -2000.25, 7},
         {1.7179869183E10, -1.7179869183E10, 0}, {1e12, 0, -1e12}, {Double.NaN, 5, 0}, {0.0784, -0.0784, 0.0}, {-0.0, 3.0517578125E-5, 0}};
      for (int i = 0; i < lp.length; i++) {
         PlayHarness.cb("set_entity_motion/" + i, new ClientboundSetEntityMotionPacket(IDS[i % IDS.length], new Vec3(lp[i][0], lp[i][1], lp[i][2])));
      }

      // links, passengers, removal
      for (int i = 0; i < IDS.length; i++) {
         PlayHarness.cb("set_entity_link/" + i, new ClientboundSetEntityLinkPacket(ent(IDS[i]), i % 3 == 0 ? null : ent(IDS[(i + 5) % IDS.length])));
      }
      PlayHarness.cb("set_entity_link/zero_source", alloc(ClientboundSetEntityLinkPacket.class, "sourceId", 0, "destId", -1));
      for (int count : new int[]{0, 1, 2, 5, 50}) {
         Entity vehicle = ent(100 + count);
         List<Entity> pass = new ArrayList<>();
         for (int i = 0; i < count; i++) {
            pass.add(ent(IDS[i % IDS.length]));
         }
         Field pf = Entity.class.getDeclaredField("passengers");
         pf.setAccessible(true);
         pf.set(vehicle, ImmutableList.copyOf(pass));
         PlayHarness.cb("set_passengers/" + count, new ClientboundSetPassengersPacket(vehicle));
         PlayHarness.cb("remove_entities/" + count, new ClientboundRemoveEntitiesPacket(pass.stream().mapToInt(Entity::getId).toArray()));
      }
      PlayHarness.cb("remove_entities/big", new ClientboundRemoveEntitiesPacket(java.util.stream.IntStream.range(0, 1000).map(i -> i * 997 - 500).toArray()));

      // mob effects: flag combos, ids, amplifier / duration extremes
      int[] effectIds = {0, 1, 5, 20, 38};
      n = 0;
      for (int mask = 0; mask < 16; mask++) {
         Holder<net.minecraft.world.effect.MobEffect> eff = BuiltInRegistries.MOB_EFFECT.asHolderIdMap().byIdOrThrow(effectIds[mask % effectIds.length]);
         int amp = mask == 15 ? 255 : mask * 7;
         int dur = mask == 14 ? -1 : mask == 13 ? Integer.MAX_VALUE : mask * 100;
         PlayHarness.cb("update_mob_effect/" + mask, new ClientboundUpdateMobEffectPacket(IDS[mask % IDS.length], new MobEffectInstance(eff, dur, amp, (mask & 1) != 0, (mask & 2) != 0, (mask & 4) != 0), (mask & 8) != 0));
         PlayHarness.cb("remove_mob_effect/" + mask, new ClientboundRemoveMobEffectPacket(IDS[mask % IDS.length], eff));
      }
      PlayHarness.cb("update_mob_effect/neg_amp", new ClientboundUpdateMobEffectPacket(3, new MobEffectInstance(MobEffects.SPEED, 20, -5, false, true, true), false));

      // attributes
      List<Holder<Attribute>> attrs = new ArrayList<>();
      for (int id : new int[]{0, 3, 7, 12}) {
         attrs.add(BuiltInRegistries.ATTRIBUTE.asHolderIdMap().byIdOrThrow(id));
      }
      attrs.add(Attributes.MAX_HEALTH);
      attrs.add(Attributes.MOVEMENT_SPEED);
      Constructor<ClientboundUpdateAttributesPacket> ctor = ClientboundUpdateAttributesPacket.class.getDeclaredConstructor(int.class, List.class);
      ctor.setAccessible(true);
      PlayHarness.cb("update_attributes/empty", ctor.newInstance(5, List.of()));
      AttributeModifier.Operation[] ops = AttributeModifier.Operation.values();
      for (int count : new int[]{1, 2, 3, 6}) {
         List<ClientboundUpdateAttributesPacket.AttributeSnapshot> snaps = new ArrayList<>();
         for (int i = 0; i < count; i++) {
            List<AttributeModifier> mods = new ArrayList<>();
            for (int m = 0; m < i; m++) {
               mods.add(new AttributeModifier(Identifier.parse(m % 2 == 0 ? "minecraft:effect.speed" : "mymod:mod/" + m), DOUBLES[(m + i) % DOUBLES.length], ops[m % ops.length]));
            }
            snaps.add(new ClientboundUpdateAttributesPacket.AttributeSnapshot(attrs.get(i % attrs.size()), DOUBLES[i % DOUBLES.length], mods));
         }
         PlayHarness.cb("update_attributes/" + count, ctor.newInstance(count * 10, snaps));
      }
      for (AttributeModifier.Operation op : ops) {
         PlayHarness.raw("raw/attribute_modifier/" + op.ordinal(), b -> ClientboundUpdateAttributesPacket.AttributeSnapshot.MODIFIER_STREAM_CODEC.encode(b, new AttributeModifier(Identifier.parse("a:b/c"), -0.5, op)));
      }
      List<ClientboundUpdateAttributesPacket.AttributeSnapshot> many = new ArrayList<>();
      for (int i = 0; i < 128; i++) {
         many.add(new ClientboundUpdateAttributesPacket.AttributeSnapshot(attrs.get(i % attrs.size()), i, List.of()));
      }
      PlayHarness.cb("update_attributes/128", ctor.newInstance(1, many));

      // player rotation
      n = 0;
      for (int i = 0; i < 4; i++) {
         PlayHarness.cb("player_rotation/" + i, new ClientboundPlayerRotationPacket(FLOATS[i + 2], (i & 1) != 0, FLOATS[i + 3], (i & 2) != 0));
      }

      // look at: every from-anchor, position and entity targets with each to-anchor
      n = 0;
      for (Anchor from : Anchor.values()) {
         PlayHarness.cb("player_look_at/pos/" + n++, new ClientboundPlayerLookAtPacket(from, 1.5, -2.5, 1e10));
         for (Anchor to : Anchor.values()) {
            PlayHarness.cb("player_look_at/entity/" + n++, alloc(ClientboundPlayerLookAtPacket.class, "fromAnchor", from, "x", 0.5, "y", 64.0, "z", -0.5, "entity", IDS[n % IDS.length], "toAnchor", to, "atEntity", true));
         }
      }
      PlayHarness.cb("player_look_at/entity0", alloc(ClientboundPlayerLookAtPacket.class, "fromAnchor", Anchor.EYES, "x", 0.0, "y", 0.0, "z", 0.0, "entity", 0, "toAnchor", Anchor.FEET, "atEntity", true));

      // move vehicle
      for (int i = 0; i < 4; i++) {
         PlayHarness.cb("move_vehicle/" + i, new ClientboundMoveVehiclePacket(PositionAndRotation.of(v(i), FLOATS[i], FLOATS[i + 3])));
      }
      PlayHarness.raw("raw/anchor/feet", b -> b.writeEnum(Anchor.FEET));
      PlayHarness.raw("raw/anchor/eyes", b -> b.writeEnum(Anchor.EYES));

      // abilities: every flag combination
      for (int mask = 0; mask < 16; mask++) {
         Abilities a = new Abilities();
         a.invulnerable = (mask & 1) != 0;
         a.flying = (mask & 2) != 0;
         a.mayfly = (mask & 4) != 0;
         a.instabuild = (mask & 8) != 0;
         a.setFlyingSpeed(FLOATS[mask % FLOATS.length]);
         a.setWalkingSpeed(FLOATS[(mask + 3) % FLOATS.length]);
         PlayHarness.cb("player_abilities/" + mask, new ClientboundPlayerAbilitiesPacket(a));
      }

      // equipment: every slot ordinal, empty and non-empty stacks, long / reversed / duplicate lists
      ItemStack[] stacks = new ItemStack[8];
      stacks[0] = new ItemStack(Items.DIAMOND_SWORD, 1);
      stacks[1] = new ItemStack(Items.STONE, 64);
      stacks[2] = ItemStack.EMPTY;
      stacks[3] = new ItemStack(Items.DIAMOND_PICKAXE, 1);
      stacks[3].setDamageValue(100);
      stacks[4] = new ItemStack(Items.STONE, 16);
      stacks[4].set(DataComponents.MAX_STACK_SIZE, 16);
      stacks[5] = new ItemStack(Items.IRON_HELMET, 1);
      stacks[5].set(DataComponents.UNBREAKABLE, Unit.INSTANCE);
      stacks[5].setDamageValue(3);
      stacks[6] = new ItemStack(Items.APPLE, 2);
      stacks[7] = new ItemStack(Items.DIAMOND_SWORD, 1);
      stacks[7].set(DataComponents.MAX_DAMAGE, 2000);
      stacks[7].setDamageValue(7);
      EquipmentSlot[] slots = EquipmentSlot.values();
      for (int i = 0; i < slots.length; i++) {
         PlayHarness.cb("set_equipment/single" + i, new ClientboundSetEquipmentPacket(IDS[i], List.of(Pair.of(slots[i], stacks[i]))));
         PlayHarness.cb("set_equipment/empty" + i, new ClientboundSetEquipmentPacket(IDS[i + 3], List.of(Pair.of(slots[i], ItemStack.EMPTY))));
      }
      List<Pair<EquipmentSlot, ItemStack>> all = new ArrayList<>();
      for (int i = 0; i < slots.length; i++) {
         all.add(Pair.of(slots[i], stacks[i]));
      }
      PlayHarness.cb("set_equipment/all", new ClientboundSetEquipmentPacket(77, all));
      List<Pair<EquipmentSlot, ItemStack>> rev = new ArrayList<>(all);
      Collections.reverse(rev);
      PlayHarness.cb("set_equipment/reversed", new ClientboundSetEquipmentPacket(-77, rev));
      PlayHarness.cb("set_equipment/two", new ClientboundSetEquipmentPacket(0, List.of(Pair.of(EquipmentSlot.HEAD, stacks[5]), Pair.of(EquipmentSlot.MAINHAND, stacks[0]))));
      PlayHarness.cb("set_equipment/dup", new ClientboundSetEquipmentPacket(5, List.of(Pair.of(EquipmentSlot.SADDLE, stacks[1]), Pair.of(EquipmentSlot.SADDLE, stacks[2]), Pair.of(EquipmentSlot.SADDLE, stacks[3]))));
      PlayHarness.cb("set_equipment/none", new ClientboundSetEquipmentPacket(5, List.of()));
      PlayHarness.close();
   }
}
