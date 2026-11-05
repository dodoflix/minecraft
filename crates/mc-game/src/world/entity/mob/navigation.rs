//! The only file of the mob slice that talks to `PathNavigation` objects: `getNavigation()` (with the controlled-vehicle
//! redirect), taking the navigation out of its mob while it runs, and the `moveTo`/`isDone`/... call shorthands goals use.

use crate::server::level::ServerLevel;
use crate::world::entity::ai::navigation::PathNavigation;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::pathfinder::{Path, PathType};
use mc_core::pos::BlockPos;

/// `getControlledVehicle()`: the vehicle when `this` is its controlling passenger.
pub fn controlled_vehicle(mob: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    let vehicle = mob.base().vehicle?;
    let v = level.get_entity(vehicle)?;
    (v.get_controlling_passenger() == Some(mob.id())).then_some(vehicle)
}

fn with_own_navigation<R>(mob: &mut dyn Entity, level: &mut ServerLevel, f: impl FnOnce(&mut dyn PathNavigation, &mut dyn Entity, &mut ServerLevel) -> R) -> Option<R> {
    let mut nav = mob.mob_mut()?.navigation.take()?;
    let result = f(&mut *nav, mob, level);
    if let Some(m) = mob.mob_mut() {
        m.navigation = Some(nav);
    }
    Some(result)
}

/// Runs `f` with `getNavigation()` of `mob` (the controlled vehicle's navigation when that is a `Mob`) taken out of its
/// owner; `f` receives the entity the navigation belongs to. `None` when there is no navigation (or it is already running).
pub fn with_navigation<R>(mob: &mut dyn Entity, level: &mut ServerLevel, f: impl FnOnce(&mut dyn PathNavigation, &mut dyn Entity, &mut ServerLevel) -> R) -> Option<R> {
    if let Some(vehicle) = controlled_vehicle(mob, level) {
        if level.get_entity(vehicle).is_some_and(|v| v.mob().is_some()) {
            return level.with_entity(vehicle, |level, v| with_own_navigation(v, level, f)).flatten();
        }
    }
    with_own_navigation(mob, level, f)
}

/// Read-only `getNavigation()` (no mutation: used by `isDone`, `canFloat`, ...).
pub fn with_navigation_ref<R>(mob: &dyn Entity, level: &ServerLevel, f: impl FnOnce(&dyn PathNavigation) -> R) -> Option<R> {
    if let Some(vehicle) = controlled_vehicle(mob, level).and_then(|id| level.get_entity(id)) {
        if let Some(m) = vehicle.mob() {
            return m.navigation.as_deref().map(f);
        }
    }
    mob.mob()?.navigation.as_deref().map(f)
}

/// `getNavigation().isDone()` (a mob without a navigation is done).
pub fn is_done(mob: &dyn Entity, level: &ServerLevel) -> bool {
    with_navigation_ref(mob, level, |n| n.is_done()).unwrap_or(true)
}

/// `getNavigation().isInProgress()`.
pub fn is_in_progress(mob: &dyn Entity, level: &ServerLevel) -> bool {
    with_navigation_ref(mob, level, |n| n.is_in_progress()).unwrap_or(false)
}

/// `getNavigation().stop()`.
pub fn stop(mob: &mut dyn Entity, level: &mut ServerLevel) {
    with_navigation(mob, level, |n, _, _| n.stop());
}

/// `getNavigation().moveTo(x, y, z, speedModifier)`.
pub fn move_to(mob: &mut dyn Entity, level: &mut ServerLevel, x: f64, y: f64, z: f64, speed_modifier: f64) -> bool {
    with_navigation(mob, level, |n, m, l| n.move_to_xyz(m, &*l, x, y, z, speed_modifier)).unwrap_or(false)
}

/// `getNavigation().moveTo(target, speedModifier)` (the target's `blockPosition()`).
pub fn move_to_entity(mob: &mut dyn Entity, level: &mut ServerLevel, target_pos: BlockPos, speed_modifier: f64) -> bool {
    with_navigation(mob, level, |n, m, l| n.move_to_entity(m, &*l, target_pos, speed_modifier)).unwrap_or(false)
}

/// `getNavigation().moveTo(path, speedModifier)`.
pub fn move_to_path(mob: &mut dyn Entity, level: &mut ServerLevel, path: Option<Path>, speed_modifier: f64) -> bool {
    with_navigation(mob, level, |n, m, l| n.move_to_path(m, &*l, path, speed_modifier)).unwrap_or(false)
}

/// `getNavigation().createPath(x, y, z, reachRange)`.
pub fn create_path_xyz(mob: &mut dyn Entity, level: &mut ServerLevel, x: f64, y: f64, z: f64, reach_range: i32) -> Option<Path> {
    with_navigation(mob, level, |n, m, l| n.create_path_xyz(m, &*l, x, y, z, reach_range)).flatten()
}

/// `getNavigation().createPath(pos, reachRange)`.
pub fn create_path_pos(mob: &mut dyn Entity, level: &mut ServerLevel, pos: BlockPos, reach_range: i32) -> Option<Path> {
    with_navigation(mob, level, |n, m, l| n.create_path_pos(m, &*l, pos, reach_range)).flatten()
}

/// `getNavigation().createPath(target, reachRange)` (the target's `blockPosition()`).
pub fn create_path_entity(mob: &mut dyn Entity, level: &mut ServerLevel, target_pos: BlockPos, reach_range: i32) -> Option<Path> {
    with_navigation(mob, level, |n, m, l| n.create_path_entity(m, &*l, target_pos, reach_range)).flatten()
}

/// `getNavigation().setSpeedModifier(speed)`.
pub fn set_speed_modifier(mob: &mut dyn Entity, level: &mut ServerLevel, speed: f64) {
    with_navigation(mob, level, |n, _, _| n.set_speed_modifier(speed));
}

/// `getNavigation().canNavigateGround()`.
pub fn can_navigate_ground(mob: &dyn Entity, level: &ServerLevel) -> bool {
    with_navigation_ref(mob, level, |n| n.can_navigate_ground()).unwrap_or(false)
}

/// `getNavigation().isStableDestination(pos)`.
pub fn is_stable_destination(mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
    with_navigation_ref(mob, level, |n| n.is_stable_destination(mob, level, pos)).unwrap_or(true)
}

/// `getNavigation().setCanFloat(canFloat)`.
pub fn set_can_float(mob: &mut dyn Entity, can_float: bool) {
    if let Some(n) = mob.mob_mut().and_then(|m| m.navigation.as_deref_mut()) {
        n.set_can_float(can_float);
    }
}

/// `getNavigation().canFloat()`.
pub fn can_float(mob: &dyn Entity, level: &ServerLevel) -> bool {
    with_navigation_ref(mob, level, |n| n.can_float()).unwrap_or(false)
}

/// `WalkNodeEvaluator.getPathTypeStatic(mob, pos)`.
pub fn path_type_static(mob: &mut dyn Entity, level: &ServerLevel, pos: BlockPos) -> PathType {
    crate::world::entity::ai::navigation::get_path_type_static_for_mob(mob, level, pos)
}

/// `MoveControl.isWalkable` core: `nodeEvaluator.getPathType(mob, pos) == WALKABLE` (true without a navigation).
pub fn is_walkable_at(mob: &mut dyn Entity, level: &mut ServerLevel, pos: BlockPos) -> bool {
    with_navigation(mob, level, |n, m, l| n.get_node_evaluator_path_type(m, &*l, pos)).is_none_or(|t| t == PathType::Walkable)
}

/// `this.navigation.stop()` on the mob's own navigation (no vehicle redirect: `TamableAnimal.maybeTeleportTo`).
pub fn with_own_navigation_stop(mob: &mut dyn Entity) {
    if let Some(n) = mob.mob_mut().and_then(|m| m.navigation.as_deref_mut()) {
        n.stop();
    }
}
