//! `net.minecraft.advancements.predicates.entity`: `EntityPredicate` and the `EntitySubPredicate` implementations.

pub mod basic;
pub mod entity_predicate;
pub mod equipment;
pub mod flags;
pub mod movement;
pub mod player_predicate;
pub mod relations;
pub mod support;
pub mod type_specific;

pub use basic::*;
pub use entity_predicate::{EntityPredicate, EntitySubPredicate, EntitySubPredicateImpl};
pub use equipment::EntityEquipmentPredicate;
pub use flags::EntityFlagsPredicate;
pub use movement::MovementPredicate;
pub use player_predicate::{AdvancementPredicate, PlayerPredicate, StatMatcher};
pub use relations::*;
pub use type_specific::*;
