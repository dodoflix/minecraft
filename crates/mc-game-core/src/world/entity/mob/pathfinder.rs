//! `PathfinderMob`: walk target value, panic state, leash following.

use super::{MobKind, leashable, navigation};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::Flag;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::LevelReader;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// `PathfinderMob.getWalkTargetValue(pos, level)` dispatched on the generic class of `e` (`Animal`, `Monster`).
pub fn get_walk_target_value(e: &dyn Entity, pos: BlockPos, level: &dyn LevelReader) -> f32 {
    let Some(m) = e.mob() else { return 0.0 };
    if m.animal.is_some() {
        super::animal::get_walk_target_value(pos, level)
    } else if m.kind == MobKind::Monster {
        -level.get_pathfinding_cost_from_light_levels(pos)
    } else {
        0.0
    }
}

/// `isPathFinding()`.
pub fn is_path_finding(e: &dyn Entity, level: &ServerLevel) -> bool {
    !navigation::is_done(e, level)
}

/// `isPanicking()`: the brain's `IS_PANICKING` memory, else a running `PanicGoal`.
pub fn is_panicking(e: &dyn Entity) -> bool {
    if let Some(by_brain) = e.is_panicking_by_brain() {
        return by_brain;
    }
    e.mob().is_some_and(|m| {
        m.goal_selector
            .is_running_where(|g| g.as_panic_goal().is_some())
    })
}

/// `PathfinderMob.closeRangeLeashBehaviour(leashHolder)`.
pub fn close_range_leash_behaviour(e: &mut dyn Entity, level: &mut ServerLevel, holder: EntityId) {
    if e.should_stay_close_to_leash_holder() && !e.is_panicking() {
        let Some(h) = level.get_entity(holder) else {
            return;
        };
        let hp = h.position();
        if let Some(m) = e.mob_mut() {
            m.goal_selector.enable_control_flag(Flag::Move);
        }
        let wanted_distance = 2.0f32;
        let p = e.position();
        let dx = hp.x - p.x;
        let dy = hp.y - p.y;
        let dz = hp.z - p.z;
        let distance_to = ((dx * dx + dy * dy + dz * dz).sqrt()) as f32;
        let delta = Vec3::new(dx, dy, dz)
            .normalize()
            .scale(f64::from((distance_to - wanted_distance).max(0.0)));
        let speed = e.follow_leash_speed();
        navigation::move_to(e, level, p.x + delta.x, p.y + delta.y, p.z + delta.z, speed);
    }
}

/// `PathfinderMob.whenLeashedTo(leashHolder)`.
pub fn when_leashed_to(e: &mut dyn Entity, level: &mut ServerLevel, holder: EntityId) {
    if let Some(h) = level.get_entity(holder) {
        let pos = h.block_position();
        let radius = e.leash_elastic_distance() as i32 - 1;
        if e.mob().is_some_and(|m| m.kind.is_pathfinder()) {
            if let Some(m) = e.mob_mut() {
                m.set_home_to(pos, radius);
            }
        }
    }
    leashable::when_leashed_to(e, level, holder);
}
