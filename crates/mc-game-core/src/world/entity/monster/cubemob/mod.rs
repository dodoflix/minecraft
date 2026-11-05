//! `net.minecraft.world.entity.monster.cubemob`: `AbstractCubeMob`, `Slime`, `MagmaCube`, `SulfurCube`.

pub mod abstract_cube_mob;
pub mod magma_cube;
pub mod slime;
pub mod sulfur_cube;

pub use abstract_cube_mob::{CubeMob, CubeMobData, CubeMobMoveControl};
