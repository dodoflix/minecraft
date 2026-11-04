//! `net.minecraft.world.entity.*`. Data inheritance by composition (`EntityData` ⊂ `LivingEntityData` ⊂ `MobData` ⊂ ...,
//! each concrete type embeds its parent's data), behavior by `trait Entity`: accessors (`base()`, `living()`, `mob()`)
//! plus overridable vanilla methods whose default impls are the vanilla super behavior; overrides call the parent
//! behavior explicitly through free functions (`entity::base_tick(self, level)`).
//!
//! Entities are `Box<dyn Entity>` in the level's `entity_manager` keyed by entity id. While an entity ticks (or while
//! a packet for a player is handled) it is *taken out* of its slot (`PersistentEntitySectionManager::take`) and put
//! back afterwards: lookups during its own tick exclude it, exactly as vanilla's level queries exclude `this`.
//! `put_back` applies what vanilla's `EntityInLevelCallback` does synchronously (section moves, removal).

pub mod attributes;
pub mod dimensions;
pub mod effect;
pub mod entity;
pub mod equipment;
pub mod factory;
pub mod interpolation;
pub mod item;
pub mod living;
pub mod movement;
pub mod plain;
pub mod player;
pub mod synched_data;
pub mod tag_set;
pub mod vec_delta_codec;

pub use dimensions::{EntityAttachment, EntityAttachments, EntityDimensions};
pub use entity::{Entity, EntityData, MoveSimulationType, MovementEmission};
pub mod respawn;
pub use player::ServerPlayer;

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

impl RemovalReason {
    pub fn should_destroy(self) -> bool {
        matches!(self, RemovalReason::Killed | RemovalReason::Discarded)
    }

    pub fn should_save(self) -> bool {
        self == RemovalReason::UnloadedToChunk
    }
}

/// Vanilla `MoverType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoverType {
    SelfMover,
    Player,
    Piston,
    ShulkerBox,
    Shulker,
}

impl MoverType {
    pub fn is_server_and_client_simulated(self) -> bool {
        self != MoverType::SelfMover
    }
}

/// Entity classes `ServerEntity` treats specially.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackingKind {
    Normal,
    /// `AbstractArrow`: always sends position + rotation.
    Arrow,
    /// `ItemEntity`: full-precision position sync on collisions.
    ItemEntity,
}
