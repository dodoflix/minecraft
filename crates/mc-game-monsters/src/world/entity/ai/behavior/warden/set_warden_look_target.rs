//! `SetWardenLookTarget`.

use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, DISTURBANCE_LOCATION, LOOK_TARGET, ROAR_TARGET,
};

/// `SetWardenLookTarget.create()`.
pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let look_target = i.registered(&LOOK_TARGET);
    let disturbance = i.registered(&DISTURBANCE_LOCATION);
    let roar_target = i.registered(&ROAR_TARGET);
    let _attack_target = i.absent(&ATTACK_TARGET);
    i.create(move |c| {
        let target = c
            .try_get(&roar_target)
            .and_then(|r| c.level.get_entity(r.0))
            .map(|e| e.block_position())
            .or_else(|| c.try_get(&disturbance).copied());
        let Some(target) = target else { return false };
        c.set(
            &look_target,
            PositionTracker::Block(BlockPosTracker::new(target)),
        );
        true
    })
}
