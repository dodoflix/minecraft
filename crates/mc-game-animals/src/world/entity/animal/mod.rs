// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-animals`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::animal::*;
pub mod equine;

pub mod axolotl;

pub mod cow;

pub mod dolphin;

pub mod feline;

pub mod fish;

pub mod fox;

pub mod golem;

pub mod frog;

pub mod nautilus;

pub mod panda;

pub mod pig;

pub mod polarbear;

pub mod rabbit;

pub mod squid;

pub mod sheep;

pub mod turtle;

pub mod wolf;
