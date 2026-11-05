//! `net.minecraft.world.entity.animal.golem`.

pub mod abstract_golem;
pub mod iron_golem;
pub mod snow_golem;

pub use iron_golem::IronGolem;
pub use snow_golem::SnowGolem;
