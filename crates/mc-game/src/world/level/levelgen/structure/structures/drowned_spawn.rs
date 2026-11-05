//! The saved NBT of a freshly spawned structure `Drowned` (`EntityTypes.DROWNED.create(.., STRUCTURE)`, persistence
//! required, `snapTo(position, 0, 0)`), as `ProtoChunk.addEntity` stores it.
//!
//! Vanilla's `finalizeSpawn` randomness (equipment, attribute bonuses, baby/left-handed) draws from the unseeded
//! `Entity.random` and the wall-clock date, so no world file reproduces it; this tag carries every field of an
//! unticked drowned with the neutral outcome of those rolls. The UUID is random in vanilla too.
// shortcut: no random equipment / attribute bonuses / baby roll, upgrade when the entity layer (Drowned + attributes) lands

use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random;
use mc_nbt::{CompoundTag, ListTag};

pub fn drowned_entity_tag(_level: &dyn WorldGenLevel, position: BlockPos) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", "minecraft:drowned");
    let mut pos = ListTag::new();
    for v in [f64::from(position.x()) + 0.5, f64::from(position.y()), f64::from(position.z()) + 0.5] {
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
    tag.put_int_array("UUID", vec![(uuid.0 >> 32) as i32, uuid.0 as i32, (uuid.1 >> 32) as i32, uuid.1 as i32]);
    tag.put_float("Health", 20.0);
    tag.put_short("HurtTime", 0);
    tag.put_short("DeathTime", 0);
    tag.put_float("AbsorptionAmount", 0.0);
    tag.put_int("current_impulse_context_reset_grace_time", 0);
    tag.put_boolean("FallFlying", false);
    tag.put_boolean("CanPickUpLoot", false);
    tag.put_boolean("PersistenceRequired", true);
    tag.put_boolean("LeftHanded", false);
    tag.put_boolean("IsBaby", false);
    tag.put_boolean("CanBreakDoors", false);
    tag.put_int("DrownedConversionTime", -1);
    tag.put_int("InWaterTime", -1);
    tag
}
