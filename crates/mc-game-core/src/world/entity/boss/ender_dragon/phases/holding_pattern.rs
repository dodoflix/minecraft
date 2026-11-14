//! `DragonHoldingPatternPhase`: circle the end island along the flight nodes; at a node now and then land or turn on the player
//! nearest to the podium.

use super::{EnderDragonPhase, random_target_above, strafe_player};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use crate::world::entity::mob::ext::get_nearest_player;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::pathfinder::path::Path;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource as _;
use mc_world::heightmap::HeightmapType;

#[derive(Default)]
pub struct State {
    /// `currentPath`.
    pub current_path: Option<Path>,
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
    /// `clockwise`.
    pub clockwise: bool,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.holding_pattern;
    state.current_path = None;
    state.target_location = None;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    let pos = dragon.base().position();
    let dist_to_target = dragon
        .phases
        .holding_pattern
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

/// `onCrystalDestroyed(crystal, pos, source, player)`.
pub fn on_crystal_destroyed(
    dragon: &mut EnderDragon,
    level: &mut ServerLevel,
    player: Option<EntityId>,
) {
    let Some(player) = player else { return };
    let can_attack = level
        .get_entity(player)
        .is_some_and(|p| dragon.can_attack(level, p));
    if can_attack {
        strafe_player(dragon, level, player);
    }
}

/// `findNewTarget(level)`.
fn find_new_target(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    if dragon
        .phases
        .holding_pattern
        .current_path
        .as_ref()
        .is_some_and(Path::is_done)
    {
        let egg = level.get_heightmap_pos(
            HeightmapType::MotionBlockingNoLeaves,
            dragon.podium_location(),
        );
        let crystals = dragon.alive_crystals().unwrap_or(0);
        if dragon.base_mut().random.next_int_bounded(crystals + 3) == 0 {
            dragon.set_phase(level, EnderDragonPhase::LandingApproach);
            return;
        }
        let conditions = TargetingConditions::for_combat().ignore_line_of_sight();
        let player = get_nearest_player(
            level,
            &conditions,
            &*dragon,
            f64::from(egg.x()),
            f64::from(egg.y()),
            f64::from(egg.z()),
        );
        let dist_sqr = match player.and_then(|id| level.get_entity(id)) {
            Some(player) => egg.dist_to_center_sqr(&player.base().position()) / 512.0,
            None => 64.0,
        };
        if let Some(player) = player {
            let random = &mut dragon.base_mut().random;
            if random.next_int_bounded((dist_sqr + 2.0) as i32) == 0
                || random.next_int_bounded(crystals + 2) == 0
            {
                strafe_player(dragon, level, player);
                return;
            }
        }
    }
    if dragon
        .phases
        .holding_pattern
        .current_path
        .as_ref()
        .is_none_or(Path::is_done)
    {
        let current_node_index = dragon.find_closest_node(level);
        let mut target_node_index = current_node_index;
        if dragon.base_mut().random.next_int_bounded(8) == 0 {
            let state = &mut dragon.phases.holding_pattern;
            state.clockwise = !state.clockwise;
            target_node_index = current_node_index + 6;
        }
        if dragon.phases.holding_pattern.clockwise {
            target_node_index += 1;
        } else {
            target_node_index -= 1;
        }
        if dragon
            .alive_crystals()
            .is_some_and(|crystals| crystals >= 0)
        {
            target_node_index %= 12;
            if target_node_index < 0 {
                target_node_index += 12;
            }
        } else {
            target_node_index -= 12;
            target_node_index &= 7;
            target_node_index += 12;
        }
        let path = dragon.find_path(current_node_index, target_node_index, None);
        if let Some(path) = &path {
            path.advance();
        }
        dragon.phases.holding_pattern.current_path = path;
    }
    navigate_to_next_path_node(dragon);
}

/// `strafePlayer(player)`.
fn strafe_player(dragon: &mut EnderDragon, level: &mut ServerLevel, player: EntityId) {
    dragon.set_phase(level, EnderDragonPhase::StrafePlayer);
    strafe_player::set_target(dragon, level, player);
}

/// `navigateToNextPathNode()`.
fn navigate_to_next_path_node(dragon: &mut EnderDragon) {
    let Some(path) = dragon.phases.holding_pattern.current_path.clone() else {
        return;
    };
    if !path.is_done() {
        let current = path.get_next_node_pos();
        path.advance();
        let target = random_target_above(&mut dragon.base_mut().random, current);
        dragon.phases.holding_pattern.target_location = Some(target);
    }
}
