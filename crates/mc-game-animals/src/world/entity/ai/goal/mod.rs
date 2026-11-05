// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-animals`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::ai::goal::*;

mod beg;

pub use beg::BegGoal;

mod llama_follow_caravan;

pub use llama_follow_caravan::LlamaFollowCaravanGoal;
