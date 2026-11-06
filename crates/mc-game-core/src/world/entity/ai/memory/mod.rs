//! `net.minecraft.world.entity.ai.memory`: the typed memory store of a [`Brain`](super::brain::Brain).
//!
//! * [`memory_module_type`] — every vanilla `MemoryModuleType<U>` as a typed constant (`memory_module_type::HOME`,
//!   registry order = declaration order, [`MemoryTypeId`] is the untyped registry index) with its codec flag.
//! * [`MemoryValue`] / [`MemoryData`] — the closed set of value kinds a memory can hold; `U` of a typed memory
//!   type is one of the `MemoryData` implementors (`bool`, `i32`, `i64`, [`Unit`], `GlobalPos`, `BlockPos`, `Vec3`,
//!   `Uuid`, [`EntityRef`], `Vec<EntityRef>`, `Vec<GlobalPos>`, [`GlobalPosSet`], `Vec<Uuid>`, [`WalkTarget`],
//!   [`PositionTracker`], [`SharedDamageSource`], `Path`, [`NearestVisibleLivingEntities`], [`SpearStatus`]).
//!   Entity-valued memories hold an [`EntityRef`] (an entity id), resolved through the level; a reference to a
//!   removed entity simply fails to resolve.
//! * [`MemorySlot`] — value + time to live (`Long.MAX_VALUE` = never expires), ticked once per brain tick.
//! * [`ExpirableValue`], [`MemoryMap`] — the persisted form (`Brain.Packed`), codec in [`codec`].

pub mod codec;
pub mod memory_map;
pub mod memory_module_type;
pub mod nearest_visible_living_entities;
pub mod slot;
pub mod value;
pub mod walk_target;

pub use memory_map::{BrainPacked, MemoryMap};
pub use memory_module_type::{MemoryModuleType, MemoryTypeId};
pub use nearest_visible_living_entities::NearestVisibleLivingEntities;
pub use slot::MemorySlot;
pub use value::{
    EntityRef, GlobalPosSet, MemoryData, MemoryValue, SharedDamageSource, SpearStatus, Unit, Void,
};
pub use walk_target::WalkTarget;

pub use crate::world::entity::ai::behavior::position_tracker::PositionTracker;

/// `MemoryStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MemoryStatus {
    ValuePresent,
    ValueAbsent,
    Registered,
}

/// `ExpirableValue<T>`: a value with an optional time to live.
#[derive(Clone, PartialEq, Debug)]
pub struct ExpirableValue<T> {
    pub value: T,
    pub time_to_live: Option<i64>,
}

impl<T> ExpirableValue<T> {
    /// `ExpirableValue.of(value)`.
    pub fn of(value: T) -> Self {
        Self {
            value,
            time_to_live: None,
        }
    }

    /// `ExpirableValue.of(value, ticksUntilExpiry)`.
    pub fn with_ttl(value: T, ticks_until_expiry: i64) -> Self {
        Self {
            value,
            time_to_live: Some(ticks_until_expiry),
        }
    }
}
