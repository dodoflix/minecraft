//! `net.minecraft.world.entity.*`. Data inheritance by composition (`EntityData` ⊂ `LivingEntityData` ⊂ `MobData` ⊂ ...,
//! each concrete type embeds its parent's data), behavior by `trait Entity`: accessors (`base()`, `living()`, `mob()`)
//! plus overridable vanilla methods whose default impls are the vanilla super behavior; overrides call the parent
//! behavior explicitly through free functions (`entity::base_tick(self, level)`).
//!
//! Entities are `Box<dyn Entity>` in the level's `entity_manager` keyed by entity id. While an entity ticks (or while
//! a packet for a player is handled) it is *taken out* of its slot (`PersistentEntitySectionManager::take`) and put
//! back afterwards: lookups during its own tick exclude it, exactly as vanilla's level queries exclude `this`.
//! `put_back` applies what vanilla's `EntityInLevelCallback` does synchronously (section moves, removal).

pub mod ai;
pub mod animal;
pub mod area_effect_cloud;
pub mod attributes;
pub mod bucketable;
pub mod crackiness;
pub mod decoration;
pub mod dimensions;
pub mod dismount_helper;
pub mod effect;
pub mod entity;
pub mod equipment;
pub mod equipment_table;
pub mod experience_orb;
pub mod factory;
pub mod falling_block_entity;
pub mod fluid_interaction;
pub mod inside_block_effects;
pub mod interaction;
pub mod interpolation;
pub mod item;
pub mod item_based_steering;
pub mod lightning_bolt;
pub mod living;
pub mod living_entity;
pub mod marker;
pub mod mob;
pub mod monster;
pub mod movement;
pub mod npc;
pub mod object_open_hash_map;
pub mod ominous_item_spawner;
pub mod plain;
pub mod player;
pub mod portal;
pub mod projectile;
pub mod raid;
pub mod spawn_group_data;
pub mod spawn_util;
pub mod sulfur_cube_archetype;
pub mod swing;
pub mod synched_data;
pub mod tag_set;
pub mod team;
pub mod variant;
pub mod vec_delta_codec;
pub mod vehicle;

pub use dimensions::{EntityAttachment, EntityAttachments, EntityDimensions};
pub use entity::{Entity, EntityData, MoveSimulationType, MovementEmission};
pub mod respawn;
pub mod schedule;
pub mod spawn_config;
pub mod spawn_placements;
pub mod spawn_predicates;

pub mod scenario_dump;
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
