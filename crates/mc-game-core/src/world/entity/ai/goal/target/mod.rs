//! `net.minecraft.world.entity.ai.goal.target`: the target-selecting goals (`TargetGoal` base embedded by each).

mod hurt_by_target_goal;
mod nearest_attackable_target_goal;
pub mod target_goal;

pub use hurt_by_target_goal::{AlertOtherFn, HurtByTargetGoal};
pub use nearest_attackable_target_goal::{NearestAttackableTargetGoal, SearchAreaFn};
pub use target_goal::TargetGoal;

use crate::world::entity::Entity;
use mc_data::EntityType;
use std::sync::Arc;

/// A Java `Class<?>` used as an `instanceof` test (`isAssignableFrom`): ignore lists, target classes.
pub type EntityClass = Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

/// `entity.getType() == type` as a class test.
pub fn class_of_type(ty: EntityType) -> EntityClass {
    Arc::new(move |e| e.get_type() == ty)
}

/// Any of `types` as a class test (`Class` of several leaf types / a type tag's members).
pub fn class_of_types(types: &'static [EntityType]) -> EntityClass {
    Arc::new(move |e| types.contains(&e.get_type()))
}

/// The concrete Rust class `T` (`T.class` for a leaf mob struct).
pub fn class_of<T: Entity>() -> EntityClass {
    Arc::new(|e| e.as_any().is::<T>())
}

/// `targetType` of `NearestAttackableTargetGoal`.
#[derive(Clone)]
pub enum TargetType {
    /// `Player.class` / `ServerPlayer.class`: looked up with `getNearestPlayer`.
    Player,
    /// Any other living class: `getEntitiesOfClass` + `getNearestEntity`.
    Class(EntityClass),
}

impl TargetType {
    pub fn of_type(ty: EntityType) -> Self {
        TargetType::Class(class_of_type(ty))
    }
}
