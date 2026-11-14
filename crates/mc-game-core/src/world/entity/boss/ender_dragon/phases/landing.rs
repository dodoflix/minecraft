//! `DragonLandingPhase`: descend onto the podium; once there, sit down and scan for players.

use super::{EnderDragonPhase, sitting_flaming};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use mc_core::phys::Vec3;
use mc_world::heightmap::HeightmapType;

#[derive(Default)]
pub struct State {
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    dragon.phases.landing.target_location = None;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    let target = match dragon.phases.landing.target_location {
        Some(target) => target,
        None => {
            let podium = level.get_heightmap_pos(
                HeightmapType::MotionBlockingNoLeaves,
                dragon.podium_location(),
            );
            let target = Vec3::at_bottom_center_of(&podium);
            dragon.phases.landing.target_location = Some(target);
            target
        }
    };
    let pos = dragon.base().position();
    if target.distance_to_sqr_xyz(pos.x, pos.y, pos.z) < 1.0 {
        sitting_flaming::reset_flame_count(dragon);
        dragon.set_phase(level, EnderDragonPhase::SittingScanning);
    }
}
