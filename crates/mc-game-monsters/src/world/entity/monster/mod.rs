// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-monsters`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::monster::*;
pub mod cubemob;

pub mod blaze;

pub mod creeper;

pub mod crossbow_attack_mob;

pub mod enderman;

pub mod endermite;

pub mod ghast;

pub mod giant;

pub mod guardian;

pub mod phantom;

pub mod shulker;

pub mod silverfish;

pub mod skeleton;

pub mod spider;

pub mod strider;

pub mod vex;

pub mod hoglin;

pub mod piglin;

pub mod zoglin;

pub mod breeze;

pub mod creaking;

pub mod warden;
