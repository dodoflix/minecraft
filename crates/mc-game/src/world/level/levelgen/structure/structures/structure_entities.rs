//! The entities worldgen structures place (`ServerLevelAccessor.addFreshEntityWithPassengers`) as the entity NBT
//! `ProtoChunk.addEntity` stores: `Entity.save` of a freshly created mob after `finalizeSpawn`. Every constructor
//! draws from the level's random in the order vanilla's `finalizeSpawn` does.

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
