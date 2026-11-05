//! `TryToSniff`.

use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::Unit;
use mc_game_core::world::entity::ai::memory::memory_module_type::{DISTURBANCE_LOCATION, IS_SNIFFING, NEAREST_ATTACKABLE, SNIFF_COOLDOWN, WALK_TARGET};
use mc_core::random::RandomSource;
use mc_data::enums::Pose;

/// `SNIFF_COOLDOWN = UniformInt.of(100, 200)`.
const SNIFF_COOLDOWN_MIN: i32 = 100;
const SNIFF_COOLDOWN_MAX: i32 = 200;

/// `TryToSniff.create()`.
pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let sniffing = i.registered(&IS_SNIFFING);
    let walk_target = i.registered(&WALK_TARGET);
    let cooldown = i.absent(&SNIFF_COOLDOWN);
    let _attackable = i.present(&NEAREST_ATTACKABLE);
    let _disturbance = i.absent(&DISTURBANCE_LOCATION);
    i.create(move |c| {
        c.set(&sniffing, Unit);
        let ticks = c.level.tick_state.random.next_int_between_inclusive(SNIFF_COOLDOWN_MIN, SNIFF_COOLDOWN_MAX);
        c.set_with_expiry(&cooldown, Unit, i64::from(ticks));
        c.erase(&walk_target);
        c.body.set_pose(Pose::Sniffing);
        true
    })
}
