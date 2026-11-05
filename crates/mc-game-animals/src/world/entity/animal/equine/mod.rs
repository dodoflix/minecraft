// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-animals`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::animal::equine::*;

pub mod abstract_chested_horse;

pub use abstract_chested_horse::AbstractChestedHorse;

pub mod donkey;

pub use donkey::Donkey;

pub mod horse;

pub use horse::Horse;

pub mod llama;

pub use llama::Llama;

pub mod llama_goals;

pub mod markings;

pub use markings::Markings;

pub mod mule;

pub use mule::Mule;

pub mod skeleton_horse;

pub use skeleton_horse::SkeletonHorse;

pub mod skeleton_trap_goal;
