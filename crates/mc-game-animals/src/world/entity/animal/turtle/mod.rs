//! `net.minecraft.world.entity.animal.turtle`.

pub mod turtle;
pub use turtle::Turtle;

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;

/// `Turtle.BABY_ON_LAND_SELECTOR`: `target.isBaby() && !target.isInWater()`.
pub fn baby_on_land_selector(target: &dyn Entity, _level: &ServerLevel) -> bool {
    target.is_baby() && !target.is_in_water()
}
