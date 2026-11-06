//! `net.minecraft.world.damagesource.*`.

pub mod combat_entry;
pub mod combat_rules;
pub mod combat_tracker;
pub mod damage_source;
pub mod damage_sources;
pub mod damage_types;
pub mod display_name;
pub mod fall_location;

pub use crate::registry::typed::damage_type::{
    DamageEffects, DamageScaling, DamageType, DeathMessageType,
};
pub use combat_entry::CombatEntry;
pub use combat_tracker::CombatTracker;
pub use damage_source::{DamageEntity, DamageSource, HurtKnockbackDirection};
pub use damage_sources::DamageSources;
pub use damage_types::{DamageTypeTags, DamageTypes};
pub use fall_location::FallLocation;
