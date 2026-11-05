//! `net.minecraft.world.entity.monster.breeze`.

#[allow(clippy::module_inception)]
pub mod breeze;
pub mod breeze_ai;
pub mod breeze_util;
pub mod long_jump;
pub mod shoot;
pub mod shoot_when_stuck;
pub mod slide;

pub use breeze::Breeze;
