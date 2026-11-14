//! `net.minecraft.world.entity.animal.golem`.

pub mod abstract_golem;
pub mod copper_golem;
mod copper_golem_ai;
pub mod iron_golem;
pub mod snow_golem;

pub use copper_golem::CopperGolem;
pub use iron_golem::IronGolem;
pub use snow_golem::SnowGolem;
