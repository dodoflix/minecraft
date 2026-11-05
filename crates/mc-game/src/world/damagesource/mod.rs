//! `net.minecraft.world.damagesource.*`.

pub mod damage_source;
pub mod damage_sources;
pub mod damage_types;

pub use crate::registry::typed::damage_type::{DamageEffects, DamageScaling, DamageType, DeathMessageType};
pub use damage_source::{DamageEntity, DamageSource};
pub use damage_sources::DamageSources;
pub use damage_types::{DamageTypeTags, DamageTypes};
