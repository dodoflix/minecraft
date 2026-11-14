//! `DragonSittingAttackingPhase`: roar for two seconds, then breathe.

use super::EnderDragonPhase;
use crate::server::level::ServerLevel;
use crate::world::entity::boss::ender_dragon::EnderDragon;

/// `ROAR_DURATION`.
const ROAR_DURATION: i32 = 40;

#[derive(Default)]
pub struct State {
    /// `attackingTicks`.
    pub attacking_ticks: i32,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    dragon.phases.sitting_attacking.attacking_ticks = 0;
}

/// `doServerTick(level)`: the roar sound itself is played by the client.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    let ticks = dragon.phases.sitting_attacking.attacking_ticks;
    dragon.phases.sitting_attacking.attacking_ticks += 1;
    if ticks >= ROAR_DURATION {
        dragon.set_phase(level, EnderDragonPhase::SittingFlaming);
    }
}
