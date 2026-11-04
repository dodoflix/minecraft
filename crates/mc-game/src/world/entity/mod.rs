//! `net.minecraft.world.entity.*`. Data inheritance by composition (`EntityData` ⊂ `LivingEntityData` ⊂ `MobData` ⊂ ...,
//! each concrete type embeds its parent's data), behavior by `trait Entity`: accessors (`base()`, `living()`, `mob()`)
//! plus overridable vanilla methods whose default impls are the vanilla super behavior; overrides call the parent
//! behavior explicitly through free functions (`entity::base_tick(self, level)`).
//!
//! Entities are `Box<dyn Entity>` in the level's entity storage keyed by entity id. While an entity ticks (or while
//! a packet for a player is handled) it is *taken out* of its slot and put back afterwards: lookups during its own
//! tick exclude it, exactly as vanilla's level queries exclude `this`.
//!
//! This is the skeleton the EntityAgent slice grows; add data fields / trait methods additively.

use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;
pub mod player;
pub use player::ServerPlayer;
use std::any::Any;
use uuid::Uuid;

pub type EntityId = i32;

/// Vanilla `Entity.RemovalReason`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemovalReason {
    Killed,
    Discarded,
    UnloadedToChunk,
    UnloadedWithPlayer,
    ChangedDimension,
}

/// Fields of vanilla `Entity` (grown by the entity slice).
pub struct EntityData {
    pub entity_type: EntityType,
    pub id: EntityId,
    pub uuid: Uuid,
    pub position: Vec3,
    pub delta_movement: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub y_rot_o: f32,
    pub x_rot_o: f32,
    pub bounding_box: AABB,
    pub on_ground: bool,
    pub tick_count: i32,
    pub removal_reason: Option<RemovalReason>,
    pub vehicle: Option<EntityId>,
    pub passengers: Vec<EntityId>,
}

/// Vanilla `Entity` and its overridable behavior.
pub trait Entity: Any {
    fn base(&self) -> &EntityData;
    fn base_mut(&mut self) -> &mut EntityData;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn id(&self) -> EntityId {
        self.base().id
    }
    fn uuid(&self) -> Uuid {
        self.base().uuid
    }
    fn get_type(&self) -> EntityType {
        self.base().entity_type
    }
    fn is_removed(&self) -> bool {
        self.base().removal_reason.is_some()
    }
}
