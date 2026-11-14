//! `DragonTakeoffPhase`: leave the podium along a path towards the far side of the island, then circle again.

use super::{EnderDragonPhase, random_target_above};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use crate::world::level::pathfinder::path::Path;
use mc_core::phys::Vec3;
use mc_world::heightmap::HeightmapType;

#[derive(Default)]
pub struct State {
    /// `firstTick`.
    pub first_tick: bool,
    /// `currentPath`.
    pub current_path: Option<Path>,
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.takeoff;
    state.first_tick = true;
    state.current_path = None;
    state.target_location = None;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    if !dragon.phases.takeoff.first_tick && dragon.phases.takeoff.current_path.is_some() {
        let egg = level.get_heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            dragon.podium_location(),
        );
        if !egg.closer_to_center_than(&dragon.base().position(), 10.0) {
            dragon.set_phase(level, EnderDragonPhase::HoldingPattern);
        }
    } else {
        dragon.phases.takeoff.first_tick = false;
        find_new_target(dragon, level);
    }
}

/// `findNewTarget()`.
fn find_new_target(dragon: &mut EnderDragon, level: &ServerLevel) {
    let current_node_index = dragon.find_closest_node(level);
    let look_vector = dragon.get_head_look_vector(level, 1.0);
    let mut target_node_index =
        dragon.find_closest_node_at(-look_vector.x * 40.0, 105.0, -look_vector.z * 40.0);
    if dragon.alive_crystals().is_some_and(|crystals| crystals > 0) {
        target_node_index %= 12;
        if target_node_index < 0 {
            target_node_index += 12;
        }
    } else {
        target_node_index -= 12;
        target_node_index &= 7;
        target_node_index += 12;
    }
    dragon.phases.takeoff.current_path =
        dragon.find_path(current_node_index, target_node_index, None);
    navigate_to_next_path_node(dragon);
}

/// `navigateToNextPathNode()`: unlike the other phases the first node (where the dragon is) is skipped here.
fn navigate_to_next_path_node(dragon: &mut EnderDragon) {
    let Some(path) = dragon.phases.takeoff.current_path.clone() else {
        return;
    };
    path.advance();
    if !path.is_done() {
        let current = path.get_next_node_pos();
        path.advance();
        let target = random_target_above(&mut dragon.base_mut().random, current);
        dragon.phases.takeoff.target_location = Some(target);
    }
}
