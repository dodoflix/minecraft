//! The entities worldgen structures place (`ServerLevelAccessor.addFreshEntityWithPassengers`) as the entity NBT
//! `ProtoChunk.addEntity` stores: `Entity.save` of a freshly created mob after `finalizeSpawn`. Every constructor
//! draws from the level's random in the order vanilla's `finalizeSpawn` does.

use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource, generate_unique_seed};
use mc_core::uuid_util::uuid_to_int_array;
use mc_nbt::{CompoundTag, ListTag};

/// `Entity.UUID`: random in vanilla as well (`Mth.createInsecureUUID` over a fresh random).
fn random_uuid() -> Vec<i32> {
    let mut random = LegacyRandomSource::new(generate_unique_seed());
    uuid_to_int_array(&mth::create_insecure_uuid(&mut random)).to_vec()
}

fn double_list(values: [f64; 3]) -> ListTag {
    let mut list = ListTag::new();
    for v in values {
        list.push(v);
    }
    list
}

/// The NBT every `LivingEntity`/`Mob` saves right after `snapTo(x, y, z, 0, 0)` + `finalizeSpawn`, without the
/// `attributes`, `Health` and mob-specific parts.
fn living_base(id: &str, pos: [f64; 3], health: f32, left_handed: bool, persistence_required: bool) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", id);
    tag.put("Pos", double_list(pos));
    tag.put("Motion", double_list([0.0; 3]));
    let mut rotation = ListTag::new();
    rotation.push(0.0f32);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_double("fall_distance", 0.0);
    tag.put_short("Fire", 0);
    tag.put_short("Air", 300);
    tag.put_boolean("OnGround", false);
    tag.put_boolean("Invulnerable", false);
    tag.put_int("PortalCooldown", 0);
    tag.put_int_array("UUID", random_uuid());
    tag.put_int("current_impulse_context_reset_grace_time", 0);
    tag.put_float("Health", health);
    tag.put_float("AbsorptionAmount", 0.0);
    tag.put_short("HurtTime", 0);
    tag.put_short("DeathTime", 0);
    tag.put_boolean("FallFlying", false);
    let mut brain = CompoundTag::new();
    brain.put("memories", CompoundTag::new());
    tag.put("Brain", brain);
    tag.put_boolean("CanPickUpLoot", false);
    tag.put_boolean("PersistenceRequired", persistence_required);
    tag.put_boolean("LeftHanded", left_handed);
    tag
}

/// One `attributes` entry: `{base, id, modifiers}`.
fn attribute(id: &str, base: f64, modifiers: Vec<CompoundTag>) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_double("base", base);
    tag.put_string("id", id);
    if !modifiers.is_empty() {
        let mut list = ListTag::new();
        for modifier in modifiers {
            list.push(modifier);
        }
        tag.put("modifiers", list);
    }
    tag
}

/// `Mob.finalizeSpawn` (the `RANDOM_SPAWN_BONUS` follow range modifier and `setLeftHanded`): draws from the level's
/// random and returns the modifier tag and the left-handedness.
fn mob_finalize_spawn(random: &mut dyn RandomSource) -> (CompoundTag, bool) {
    let mut modifier = CompoundTag::new();
    modifier.put_double("amount", random.triangle(0.0, 0.11485000000000001));
    modifier.put_string("id", "minecraft:random_spawn_bonus");
    modifier.put_string("operation", "add_multiplied_base");
    let left_handed = random.next_float() < 0.05;
    (modifier, left_handed)
}

/// `OceanMonumentPiece.spawnElder`: `ELDER_GUARDIAN.create`, `heal(maxHealth)`, `snapTo(x + 0.5, y, z + 0.5, 0, 0)`,
/// `finalizeSpawn` (guardians keep the base `Mob.finalizeSpawn`).
pub fn elder_guardian(random: &mut dyn RandomSource, pos: BlockPos) -> CompoundTag {
    let (modifier, left_handed) = mob_finalize_spawn(random);
    let mut tag = living_base("minecraft:elder_guardian", [pos.x() as f64 + 0.5, pos.y() as f64, pos.z() as f64 + 0.5], 80.0, left_handed, true);
    let mut attributes = ListTag::new();
    attributes.push(attribute("minecraft:follow_range", 16.0, vec![modifier]));
    tag.put("attributes", attributes);
    tag
}

/// The illager part of the saved NBT (`Raider`/`PatrollingMonster`/`AbstractIllager`): not joined to a raid, no patrol.
fn illager_base(id: &str, pos: BlockPos, health: f32, left_handed: bool) -> CompoundTag {
    let mut tag = living_base(id, [pos.x() as f64 + 0.5, pos.y() as f64, pos.z() as f64 + 0.5], health, left_handed, true);
    tag.put_boolean("CanJoinRaid", true);
    tag.put_boolean("PatrolLeader", false);
    tag.put_boolean("Patrolling", false);
    tag.put_int("Wave", 0);
    tag
}

fn follow_range(base: f64, modifier: CompoundTag) -> ListTag {
    let mut attributes = ListTag::new();
    attributes.push(attribute("minecraft:follow_range", base, vec![modifier]));
    attributes
}

/// `WoodlandMansionPiece.handleDataMarker` "Mage": `EVOKER.create`, `setPersistenceRequired`, `snapTo(pos, 0, 0)`,
/// `finalizeSpawn` (`Mob`: follow range bonus + handedness; `Raider`/`PatrollingMonster` draw nothing for `STRUCTURE`).
pub fn evoker(random: &mut dyn RandomSource, pos: BlockPos) -> CompoundTag {
    let (modifier, left_handed) = mob_finalize_spawn(random);
    let mut tag = illager_base("minecraft:evoker", pos, 24.0, left_handed);
    tag.put_int("SpellTicks", 0);
    tag.put("attributes", follow_range(12.0, modifier));
    tag
}

/// "Warrior": as [`evoker`] plus `Vindicator.finalizeSpawn`: the iron axe and the (drawn) enchantment roll. The roll
/// succeeds with `0.25 * difficulty.getSpecialMultiplier()`; the enchantment itself (`EnchantmentHelper.enchantItemFromProvider`
/// with `mob_spawn_equipment`) is not ported, the axe stays plain.
pub fn vindicator(random: &mut dyn RandomSource, pos: BlockPos, special_multiplier: f32) -> CompoundTag {
    let (modifier, left_handed) = mob_finalize_spawn(random);
    let mut tag = illager_base("minecraft:vindicator", pos, 24.0, left_handed);
    tag.put("attributes", follow_range(12.0, modifier));
    let mut axe = CompoundTag::new();
    axe.put_int("count", 1);
    axe.put_string("id", "minecraft:iron_axe");
    let mut equipment = CompoundTag::new();
    equipment.put("mainhand", axe);
    tag.put("equipment", equipment);
    let _enchant_roll = random.next_float() < 0.25f32 * special_multiplier;
    tag
}

/// "Group of Allays": `ALLAY.create` + `finalizeSpawn` (`Mob`).
pub fn allay(random: &mut dyn RandomSource, pos: BlockPos) -> CompoundTag {
    let (modifier, left_handed) = mob_finalize_spawn(random);
    let mut tag = living_base("minecraft:allay", [pos.x() as f64 + 0.5, pos.y() as f64, pos.z() as f64 + 0.5], 20.0, left_handed, true);
    tag.put_long("DuplicationCooldown", 0);
    tag.put("Inventory", ListTag::new());
    let mut selector = CompoundTag::new();
    selector.put_long("tick", -1);
    let mut listener = CompoundTag::new();
    listener.put_int("event_delay", 0);
    listener.put("selector", selector);
    tag.put("listener", listener);
    tag.put("attributes", follow_range(16.0, modifier));
    tag
}

/// `EndCityPiece` "Sentry": `SHULKER.create(level, STRUCTURE)` + `setPos(x + 0.5, y, z + 0.5)` (no `finalizeSpawn`, no
/// `snapTo`: the yaw is `LivingEntity`'s `Math.random()` roll, as in vanilla).
pub fn shulker(pos: BlockPos) -> CompoundTag {
    let mut tag = living_base("minecraft:shulker", [pos.x() as f64 + 0.5, pos.y() as f64, pos.z() as f64 + 0.5], 30.0, false, false);
    let mut random = LegacyRandomSource::new(generate_unique_seed());
    let yaw = (random.next_double() * 6.2831854820251465) as f32;
    let mut rotation = ListTag::new();
    rotation.push(yaw);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_byte("AttachFace", 0);
    tag.put_byte("Color", 16);
    tag.put_byte("Peek", 0);
    tag.put("attributes", ListTag::new());
    tag
}

/// `new ItemFrame(level, pos, direction)` + `setItem(new ItemStack(Items.ELYTRA), false)`, as the saved NBT
/// (`HangingEntity`/`ItemFrame.addAdditionalSaveData`; `direction` is horizontal).
pub fn elytra_item_frame(pos: BlockPos, direction: Direction) -> CompoundTag {
    // `createBoundingBox`: `AABB.ofSize(atCenterOf(pos).relative(direction, -0.46875), ...)`, its centre is the entity position.
    let center = |c: i32, step: i32, size: f64| {
        let middle = c as f64 + 0.5 + f64::from(step) * -0.46875;
        let (min, max) = (middle - size / 2.0, middle + size / 2.0);
        min + (max - min) * 0.5
    };
    let size = |axis: Axis| if direction.get_axis() == axis { 0.0625 } else { f64::from(0.75f32) };
    let mut tag = CompoundTag::new();
    tag.put_string("id", "minecraft:item_frame");
    tag.put("Pos", double_list([center(pos.x(), direction.get_step_x(), size(Axis::X)), center(pos.y(), direction.get_step_y(), size(Axis::Y)), center(pos.z(), direction.get_step_z(), size(Axis::Z))]));
    tag.put("Motion", double_list([0.0; 3]));
    let mut rotation = ListTag::new();
    rotation.push((direction.get_2d_data_value() * 90) as f32);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_double("fall_distance", 0.0);
    tag.put_short("Fire", 0);
    tag.put_short("Air", 300);
    tag.put_boolean("OnGround", false);
    tag.put_boolean("Invulnerable", false);
    tag.put_int("PortalCooldown", 0);
    tag.put_int_array("UUID", random_uuid());
    tag.put_int_array("block_pos", vec![pos.x(), pos.y(), pos.z()]);
    let mut item = CompoundTag::new();
    item.put_int("count", 1);
    item.put_string("id", "minecraft:elytra");
    tag.put("Item", item);
    tag.put_byte("ItemRotation", 0);
    tag.put_float("ItemDropChance", 1.0);
    tag.put_byte("Facing", direction.get_3d_data_value() as i8);
    tag.put_boolean("Invisible", false);
    tag.put_boolean("Fixed", false);
    tag
}
