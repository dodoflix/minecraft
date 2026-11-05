//! `net.minecraft.world.entity.animal.turtle`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;

/// `Turtle.BABY_ON_LAND_SELECTOR`: `target.isBaby() && !target.isInWater()`.
pub fn baby_on_land_selector(target: &dyn Entity, _level: &ServerLevel) -> bool {
    target.is_baby() && !target.is_in_water()
}
