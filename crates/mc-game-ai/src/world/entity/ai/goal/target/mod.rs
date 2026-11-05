// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-ai`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::ai::goal::target::*;

mod defend_village_target_goal;

pub use defend_village_target_goal::DefendVillageTargetGoal;

mod non_tame_random_target_goal;

pub use non_tame_random_target_goal::NonTameRandomTargetGoal;

mod owner_hurt_by_target_goal;

pub use owner_hurt_by_target_goal::OwnerHurtByTargetGoal;

mod owner_hurt_target_goal;

pub use owner_hurt_target_goal::OwnerHurtTargetGoal;

mod nearest_attackable_witch_target_goal;

pub use nearest_attackable_witch_target_goal::NearestAttackableWitchTargetGoal;
