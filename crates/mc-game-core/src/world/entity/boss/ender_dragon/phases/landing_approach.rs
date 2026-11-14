//! `DragonLandingApproachPhase`: fly a path that ends at the podium, then land.

use super::{EnderDragonPhase, random_target_above};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use crate::world::entity::mob::ext::get_nearest_player;
use crate::world::level::pathfinder::node::Node;
use crate::world::level::pathfinder::path::Path;
use mc_core::phys::Vec3;
use mc_world::heightmap::HeightmapType;

#[derive(Default)]
pub struct State {
    /// `currentPath`.
    pub current_path: Option<Path>,
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.landing_approach;
    state.current_path = None;
    state.target_location = None;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    let pos = dragon.base().position();
    let dist_to_target = dragon
        .phases
        .landing_approach
        .target_location
        .map_or(0.0, |target| {
            target.distance_to_sqr_xyz(pos.x, pos.y, pos.z)
        });
    if dist_to_target < 100.0
        || dist_to_target > 22500.0
        || dragon.base().horizontal_collision
        || dragon.base().vertical_collision
    {
        find_new_target(dragon, level);
    }
}

/// `findNewTarget(level)`.
fn find_new_target(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    if dragon
        .phases
        .landing_approach
        .current_path
        .as_ref()
        .is_none_or(Path::is_done)
    {
        let current_node_index = dragon.find_closest_node(level);
        let egg = level.get_heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            dragon.podium_location(),
        );
        let conditions = TargetingConditions::for_combat().ignore_line_of_sight();
        let player = get_nearest_player(
            level,
            &conditions,
            &*dragon,
            f64::from(egg.x()),
            f64::from(egg.y()),
            f64::from(egg.z()),
        );
        let target_node_index = match player.and_then(|id| level.get_entity(id)) {
            Some(player) => {
                let aim = Vec3::new(player.base().x(), 0.0, player.base().z()).normalize();
                dragon.find_closest_node_at(-aim.x * 40.0, 105.0, -aim.z * 40.0)
            }
            None => dragon.find_closest_node_at(40.0, f64::from(egg.y()), 0.0),
        };
        let final_node = Node::new(egg.x(), egg.y(), egg.z());
        let path = dragon.find_path(current_node_index, target_node_index, Some(final_node));
        if let Some(path) = &path {
            path.advance();
        }
        dragon.phases.landing_approach.current_path = path;
    }
    navigate_to_next_path_node(dragon);
    if dragon
        .phases
        .landing_approach
        .current_path
        .as_ref()
        .is_some_and(Path::is_done)
    {
        dragon.set_phase(level, EnderDragonPhase::Landing);
    }
}

/// `navigateToNextPathNode()`.
fn navigate_to_next_path_node(dragon: &mut EnderDragon) {
    let Some(path) = dragon.phases.landing_approach.current_path.clone() else {
        return;
    };
    if !path.is_done() {
        let current = path.get_next_node_pos();
        path.advance();
        let target = random_target_above(&mut dragon.base_mut().random, current);
        dragon.phases.landing_approach.target_location = Some(target);
    }
}
