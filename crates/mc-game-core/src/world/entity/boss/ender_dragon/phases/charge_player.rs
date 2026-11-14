//! `DragonChargePlayerPhase`: ram a spot at speed 3 and recover for ten ticks.

use super::EnderDragonPhase;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::boss::ender_dragon::EnderDragon;
use mc_core::phys::Vec3;

/// `CHARGE_RECOVERY_TIME`.
const CHARGE_RECOVERY_TIME: i32 = 10;

#[derive(Default)]
pub struct State {
    /// `targetLocation`.
    pub target_location: Option<Vec3>,
    /// `timeSinceCharge`.
    pub time_since_charge: i32,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.charge_player;
    state.target_location = None;
    state.time_since_charge = 0;
}

/// `setTarget(target)`.
pub fn set_target(dragon: &mut EnderDragon, target: Vec3) {
    dragon.phases.charge_player.target_location = Some(target);
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    let Some(target) = dragon.phases.charge_player.target_location else {
        log::warn!("Aborting charge player as no target was set.");
        dragon.set_phase(level, EnderDragonPhase::HoldingPattern);
        return;
    };
    let state = &mut dragon.phases.charge_player;
    let recovered = state.time_since_charge > 0 && {
        state.time_since_charge += 1;
        state.time_since_charge > CHARGE_RECOVERY_TIME
    };
    if recovered {
        dragon.set_phase(level, EnderDragonPhase::HoldingPattern);
    } else {
        let pos = dragon.base().position();
        let dist_to_target = target.distance_to_sqr_xyz(pos.x, pos.y, pos.z);
        if dist_to_target < 100.0
            || dist_to_target > 22500.0
            || dragon.base().horizontal_collision
            || dragon.base().vertical_collision
        {
            dragon.phases.charge_player.time_since_charge += 1;
        }
    }
}
