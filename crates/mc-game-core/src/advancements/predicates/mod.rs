//! `net.minecraft.advancements.predicates`: the data-driven predicates shared by loot conditions, enchantment effects,
//! advancement triggers and commands.
//!
//! The data classes whose codecs already live in `mc_protocol::components` (`MinMaxBounds` = [`Ints`]/[`Doubles`],
//! `NbtPredicate`, `StatePropertiesPredicate`, `BlockPredicate`, `DataComponentMatchers`, `ItemPredicate`,
//! `CollectionPredicate`, `EnchantmentPredicate`, `MobEffectsPredicate`) are re-exported; their evaluation against our
//! level/entity model is added by the `*Ext` traits of this module.
//!
//! Entities are always `&dyn Entity` handed in by the caller (the entity that is currently running is taken out of the
//! level's storage, so it can never be found by id): `EntityPredicate::matches(level, origin, entity)`.

pub mod block_predicate;
pub mod damage_predicate;
pub mod damage_source_predicate;
pub mod distance_predicate;
pub mod entity;
pub mod fluid_predicate;
pub mod food_predicate;
pub mod game_type_predicate;
pub mod input_predicate;
pub mod light_predicate;
pub mod location_predicate;
pub mod mob_effects_predicate;
pub mod nbt_predicate;
pub mod slots_predicate;
pub mod tag_predicate;

pub use block_predicate::BlockPredicateLevelExt;
pub use damage_predicate::DamagePredicate;
pub use damage_source_predicate::DamageSourcePredicate;
pub use distance_predicate::DistancePredicate;
pub use entity::{EntityEquipmentPredicate, EntityPredicate, EntitySubPredicate, MovementPredicate, PlayerPredicate};
pub use fluid_predicate::{FluidPredicate, StatePropertiesPredicateFluidExt};
pub use food_predicate::FoodPredicate;
pub use game_type_predicate::GameTypePredicate;
pub use input_predicate::InputPredicate;
pub use light_predicate::LightPredicate;
pub use location_predicate::{LocationPredicate, PositionPredicate};
pub use mc_protocol::components::{
    BlockPredicate, CollectionPredicate, DataComponentMatchers, Doubles, EnchantmentPredicate, FloatDegrees, Ints, ItemPredicate, MobEffectsPredicate,
    NbtPredicate, StatePropertiesPredicate,
};
pub use mob_effects_predicate::MobEffectsPredicateEntityExt;
pub use nbt_predicate::NbtPredicateEntityExt;
pub use slots_predicate::SlotsPredicate;
pub use tag_predicate::TagPredicate;
