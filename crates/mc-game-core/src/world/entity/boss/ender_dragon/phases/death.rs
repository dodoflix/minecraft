//! `DragonDeathPhase`: fly to the podium at speed 3; within ten blocks of it the dragon's health drops to zero and the death
//! animation starts.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use mc_core::phys::Vec3;
use mc_world::heightmap::HeightmapType;

#[derive(Default)]
pub struct State {
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
    /// `time`.
    pub time: i32,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.dying;
    state.target_location = None;
    state.time = 0;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    dragon.phases.dying.time += 1;
    let target = match dragon.phases.dying.target_location {
        Some(target) => target,
        None => {
            let egg =
                level.get_heightmap_pos(HeightmapType::MotionBlocking, dragon.podium_location());
            let target = Vec3::at_bottom_center_of(&egg);
            dragon.phases.dying.target_location = Some(target);
            target
        }
    };
    let pos = dragon.base().position();
    let dist_to_target = target.distance_to_sqr_xyz(pos.x, pos.y, pos.z);
    let health = if !(dist_to_target < 100.0)
        && !(dist_to_target > 22500.0)
        && !dragon.base().horizontal_collision
        && !dragon.base().vertical_collision
    {
        1.0
    } else {
        0.0
    };
    dragon.set_health(health);
}
