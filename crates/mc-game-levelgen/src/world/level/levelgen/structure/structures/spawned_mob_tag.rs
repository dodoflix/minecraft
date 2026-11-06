//! The saved NBT of the mobs structure pieces spawn while placing (`Drowned` of the ocean ruins, `Witch` and `Cat` of the
//! swamp hut): `EntityTypes.X.create(level, STRUCTURE)`, persistence required, `snapTo(position, 0, 0)`,
//! `finalizeSpawn`, as `ProtoChunk.addEntity` stores them.
//!
//! These are the tags of an unticked mob with the fields that do not depend on
//! `finalizeSpawn`'s randomness. Vanilla draws that randomness (attribute bonuses, left-handedness, equipment, baby/jockey
//! rolls, cat variant) partly from the region random and the world difficulty/clock, and partly from the unseeded
//! `Entity.random`; the UUID is random too, so no world file reproduces them.
// Limitation: no finalizeSpawn outcome (attributes, equipment, cat variant)

use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random;
use mc_nbt::{CompoundTag, ListTag};

/// `Entity.saveWithoutId` + `LivingEntity` + `Mob` fields of a mob standing at `position`.
fn mob_tag(id: &str, position: BlockPos, max_health: f32) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", id);
    let mut pos = ListTag::new();
    for v in [
        f64::from(position.x()) + 0.5,
        f64::from(position.y()),
        f64::from(position.z()) + 0.5,
    ] {
        pos.push(v);
    }
    tag.put("Pos", pos);
    let mut motion = ListTag::new();
    for _ in 0..3 {
        motion.push(0.0f64);
    }
    tag.put("Motion", motion);
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
    let uuid = mth::create_insecure_uuid(&mut random::create_new()).as_u64_pair();
    tag.put_int_array(
        "UUID",
        vec![
            (uuid.0 >> 32) as i32,
            uuid.0 as i32,
            (uuid.1 >> 32) as i32,
            uuid.1 as i32,
        ],
    );
    tag.put_float("Health", max_health);
    tag.put_short("HurtTime", 0);
    tag.put_short("DeathTime", 0);
    tag.put_float("AbsorptionAmount", 0.0);
    tag.put_int("current_impulse_context_reset_grace_time", 0);
    tag.put_boolean("FallFlying", false);
    tag.put("Brain", {
        let mut brain = CompoundTag::new();
        brain.put("memories", CompoundTag::new());
        brain
    });
    tag.put("attributes", ListTag::new());
    tag.put_boolean("CanPickUpLoot", false);
    tag.put_boolean("PersistenceRequired", true);
    tag.put_boolean("LeftHanded", false);
    tag
}

/// A structure `Drowned` (`Zombie` + `Drowned` fields).
pub fn drowned_entity_tag(position: BlockPos) -> CompoundTag {
    let mut tag = mob_tag("minecraft:drowned", position, 20.0);
    tag.put_boolean("IsBaby", false);
    tag.put_boolean("CanBreakDoors", false);
    tag.put_int("DrownedConversionTime", -1);
    tag.put_int("InWaterTime", -1);
    tag
}

/// The swamp hut `Witch` (`Raider` fields).
pub fn witch_entity_tag(position: BlockPos) -> CompoundTag {
    let mut tag = mob_tag("minecraft:witch", position, 26.0);
    tag.put_boolean("CanJoinRaid", true); // `Raider.finalizeSpawn`: true unless a witch from a spawner
    tag.put_boolean("PatrolLeader", false);
    tag.put_boolean("Patrolling", false);
    tag.put_int("Wave", 0);
    tag
}

/// The swamp hut `Cat` (`Animal`/`TamableAnimal` fields; a swamp hut cat spawns as `black`).
pub fn cat_entity_tag(position: BlockPos) -> CompoundTag {
    let mut tag = mob_tag("minecraft:cat", position, 10.0);
    tag.put_int("Age", 0);
    tag.put_boolean("AgeLocked", false);
    tag.put_int("ForcedAge", 0);
    tag.put_int("InLove", 0);
    tag.put_boolean("Sitting", false);
    tag.put_byte("CollarColor", 14);
    tag.put_string("variant", "minecraft:black");
    tag.put_string("sound_variant", "minecraft:classic");
    tag
}
