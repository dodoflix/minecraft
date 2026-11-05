//! Spawning of the mobs hatched by blocks (`TurtleEggBlock`, `FrogspawnBlock`, `SnifferEggBlock`, `DriedGhastBlock`).
//!
//! Vanilla creates the mob with `EntityType.create(level, EntitySpawnReason.BREEDING)`, configures it through typed
//! setters (`setAge`, `setHomePos`, `setBaby`, `snapTo`, ...) and adds it with `addFreshEntity`. The configuration is
//! applied here as the equivalent saved-NBT fields (`Pos`, `Rotation`, `Age`, `home_pos`, `PersistenceRequired`),
//! which every mob class reads through its `load`, so it behaves the same for any entity class of the type.

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::{CompoundTag, ListTag, Tag};

/// `AgeableMob.setBaby(true)` / `Turtle.setAge(-24000)`: the `Age` field written by `AgeableMob.addAdditionalSaveData`.
pub const BABY_AGE: i32 = -24000;

/// `entity.snapTo(x, y, z, yRot, xRot)` as saved fields.
pub fn snap_to_tag(x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32) -> CompoundTag {
    let mut tag = CompoundTag::new();
    let mut pos = ListTag::new();
    pos.push(Tag::double(x));
    pos.push(Tag::double(y));
    pos.push(Tag::double(z));
    tag.put("Pos", pos);
    let mut rotation = ListTag::new();
    rotation.push(Tag::float(y_rot));
    rotation.push(Tag::float(x_rot));
    tag.put("Rotation", rotation);
    tag
}

/// `EntityType.create(level, BREEDING)` + the configuration in `tag` + `level.addFreshEntity(entity)`: the new entity.
pub fn spawn_breeding(level: &mut ServerLevel, ty: EntityType, tag: &CompoundTag) -> Option<EntityId> {
    let Some(mut entity) = factory::create(ty, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false }) else {
        return None;
    };
    if let Err(message) = entity.load(tag) {
        log::warn!("Exception configuring hatched {}: {message}", ty.name());
        return None;
    }
    let id = entity.id();
    level.add_fresh_entity(entity).then_some(id)
}
