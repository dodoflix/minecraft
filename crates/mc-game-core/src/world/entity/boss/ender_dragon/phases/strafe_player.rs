//! `DragonStrafePlayerPhase`: fly along a path to a player and spit a dragon fireball at it.

use super::{EnderDragonPhase, random_target_above};
use crate::server::level::ServerLevel;
use crate::world::entity::boss::ender_dragon::{EnderDragon, HEAD};
use crate::world::entity::mob::ext::has_line_of_sight;
use crate::world::entity::projectile::hurting::dragon_fireball::DragonFireball;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::LevelAccessor;
use crate::world::level::pathfinder::node::Node;
use crate::world::level::pathfinder::path::Path;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource as _;

/// `FIREBALL_CHARGE_AMOUNT`.
const FIREBALL_CHARGE_AMOUNT: i32 = 5;
/// `DEG_TO_RAD`.
const DEG_TO_RAD: f32 = 0.017453292;

#[derive(Default)]
pub struct State {
    /// `fireballCharge`.
    pub fireball_charge: i32,
    /// `currentPath`.
    pub current_path: Option<Path>,
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
    /// `attackTarget`.
    pub attack_target: Option<EntityId>,
    /// `holdingPatternClockwise`.
    pub holding_pattern_clockwise: bool,
}

/// `begin()`: the turn direction stays.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.strafe_player;
    state.fireball_charge = 0;
    state.target_location = None;
    state.current_path = None;
    state.attack_target = None;
}

/// `setTarget(target)`.
pub fn set_target(dragon: &mut EnderDragon, level: &mut ServerLevel, target: EntityId) {
    // shortcut: a target that left the level reads as no target, vanilla keeps reading the stale entity
    let Some(target_entity) = level.get_entity(target) else {
        return;
    };
    let (target_x, target_y, target_z) = (
        target_entity.base().x(),
        target_entity.base().y(),
        target_entity.base().z(),
    );
    let (target_block_x, target_block_z) = (
        target_entity.base().get_block_x(),
        target_entity.base().get_block_z(),
    );
    dragon.phases.strafe_player.attack_target = Some(target);
    let current_node_index = dragon.find_closest_node(level);
    let target_node_index = dragon.find_closest_node_at(target_x, target_y, target_z);
    let xd = f64::from(target_block_x) - dragon.base().x();
    let zd = f64::from(target_block_z) - dragon.base().z();
    let sd = (xd * xd + zd * zd).sqrt();
    let ho = (f64::from(0.4f32) + sd / 80.0 - 1.0).min(10.0);
    let final_y_target = mth::floor_f64(target_y + ho);
    let final_node = Node::new(target_block_x, final_y_target, target_block_z);
    let path = dragon.find_path(current_node_index, target_node_index, Some(final_node));
    if let Some(path) = &path {
        path.advance();
    }
    dragon.phases.strafe_player.current_path = path;
    navigate_to_next_path_node(dragon);
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    let Some(target) = dragon.phases.strafe_player.attack_target else {
        log::warn!("Skipping player strafe phase because no player was found");
        dragon.set_phase(level, EnderDragonPhase::HoldingPattern);
        return;
    };
    // shortcut: a target that left the level reads as no target, vanilla keeps reading the stale entity
    let Some(target_entity) = level.get_entity(target) else {
        dragon.phases.strafe_player.attack_target = None;
        return;
    };
    let (target_x, target_y, target_z) = (
        target_entity.base().x(),
        target_entity.base().y(),
        target_entity.base().z(),
    );
    let dragon_pos = dragon.base().position();
    if dragon
        .phases
        .strafe_player
        .current_path
        .as_ref()
        .is_some_and(Path::is_done)
    {
        let x_target_dist = target_x - dragon_pos.x;
        let z_target_dist = target_z - dragon_pos.z;
        let dist = (x_target_dist * x_target_dist + z_target_dist * z_target_dist).sqrt();
        let height_offset = (f64::from(0.4f32) + dist / 80.0 - 1.0).min(10.0);
        dragon.phases.strafe_player.target_location =
            Some(Vec3::new(target_x, target_y + height_offset, target_z));
    }
    let dist_to_target = dragon
        .phases
        .strafe_player
        .target_location
        .map_or(0.0, |t| {
            t.distance_to_sqr_xyz(dragon_pos.x, dragon_pos.y, dragon_pos.z)
        });
    if dist_to_target < 100.0 || dist_to_target > 22500.0 {
        find_new_target(dragon, level);
    }
    let in_range = level
        .get_entity(target)
        .is_some_and(|t| t.base().distance_to_sqr_vec(dragon.base().position()) < 4096.0);
    if in_range {
        let sees = level
            .get_entity(target)
            .is_some_and(|t| has_line_of_sight(level, &*dragon, t));
        if sees {
            dragon.phases.strafe_player.fireball_charge += 1;
            let aim = Vec3::new(
                target_x - dragon.base().x(),
                0.0,
                target_z - dragon.base().z(),
            )
            .normalize();
            let y_rot = dragon.base().y_rot();
            let dir = Vec3::new(
                f64::from(mth::sin(f64::from(y_rot * DEG_TO_RAD))),
                0.0,
                f64::from(-mth::cos(f64::from(y_rot * DEG_TO_RAD))),
            )
            .normalize();
            let dot = dir.dot(aim) as f32;
            let mut angle_degs = (mth::java_acos(f64::from(dot)) * 57.2957763671875) as f32;
            angle_degs += 0.5;
            if dragon.phases.strafe_player.fireball_charge >= FIREBALL_CHARGE_AMOUNT
                && angle_degs >= 0.0
                && angle_degs < 10.0
            {
                shoot_fireball(dragon, level, target);
            }
        } else if dragon.phases.strafe_player.fireball_charge > 0 {
            dragon.phases.strafe_player.fireball_charge -= 1;
        }
    } else if dragon.phases.strafe_player.fireball_charge > 0 {
        dragon.phases.strafe_player.fireball_charge -= 1;
    }
}

/// The fireball of `doServerTick`: aimed from just behind the head at the middle of the target.
fn shoot_fireball(dragon: &mut EnderDragon, level: &mut ServerLevel, target: EntityId) {
    let view_vector = dragon.get_view_vector(1.0);
    let Some(head) = level.dragon_part(dragon.part_ids()[HEAD]) else {
        return;
    };
    let starting_x = head.base().x() - view_vector.x * 1.0;
    let starting_y = head.base().get_y_progress(0.5) + 0.5;
    let starting_z = head.base().z() - view_vector.z * 1.0;
    let Some(target_entity) = level.get_entity(target) else {
        return;
    };
    let xdd = target_entity.base().x() - starting_x;
    let ydd = target_entity.base().get_y_progress(0.5) - starting_y;
    let zdd = target_entity.base().z() - starting_z;
    let direction = Vec3::new(xdd, ydd, zdd);
    if !dragon.base().is_silent() {
        level.level_event(None, 1017, dragon.base().block_position(), 0);
    }
    let mut fireball = DragonFireball::from_mob(level, &*dragon, direction.normalize());
    fireball.snap_to(starting_x, starting_y, starting_z, 0.0, 0.0);
    level.add_fresh_entity(Box::new(fireball));
    dragon.phases.strafe_player.fireball_charge = 0;
    if let Some(path) = &dragon.phases.strafe_player.current_path {
        while !path.is_done() {
            path.advance();
        }
    }
    dragon.set_phase(level, EnderDragonPhase::HoldingPattern);
}

/// `findNewTarget()`.
fn find_new_target(dragon: &mut EnderDragon, level: &ServerLevel) {
    if dragon
        .phases
        .strafe_player
        .current_path
        .as_ref()
        .is_none_or(Path::is_done)
    {
        let current_node_index = dragon.find_closest_node(level);
        let mut target_node_index = current_node_index;
        if dragon.base_mut().random.next_int_bounded(8) == 0 {
            let state = &mut dragon.phases.strafe_player;
            state.holding_pattern_clockwise = !state.holding_pattern_clockwise;
            target_node_index = current_node_index + 6;
        }
        if dragon.phases.strafe_player.holding_pattern_clockwise {
            target_node_index += 1;
        } else {
            target_node_index -= 1;
        }
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
        let path = dragon.find_path(current_node_index, target_node_index, None);
        if let Some(path) = &path {
            path.advance();
        }
        dragon.phases.strafe_player.current_path = path;
    }
    navigate_to_next_path_node(dragon);
}

/// `navigateToNextPathNode()`.
fn navigate_to_next_path_node(dragon: &mut EnderDragon) {
    let Some(path) = dragon.phases.strafe_player.current_path.clone() else {
        return;
    };
    if !path.is_done() {
        let current = path.get_next_node_pos();
        path.advance();
        let target = random_target_above(&mut dragon.base_mut().random, current);
        dragon.phases.strafe_player.target_location = Some(target);
    }
}
