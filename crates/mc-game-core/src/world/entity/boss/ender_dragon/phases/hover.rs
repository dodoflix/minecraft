//! `DragonHoverPhase`: stay where the phase began (the phase of a fresh dragon).

use crate::world::entity::Entity;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use mc_core::phys::Vec3;

#[derive(Default)]
pub struct State {
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    dragon.phases.hover.target_location = None;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon) {
    if dragon.phases.hover.target_location.is_none() {
        dragon.phases.hover.target_location = Some(dragon.base().position());
    }
}
