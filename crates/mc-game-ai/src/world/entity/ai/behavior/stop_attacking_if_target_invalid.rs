//! `StopAttackingIfTargetInvalid`.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE};
use mc_game_core::world::entity::entity::Entity;

/// `TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE`.
const TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE: i64 = 200;

/// `create(onTargetErased)`.
pub fn create_on_erased(on_target_erased: impl FnMut(&mut ServerLevel, &mut dyn Entity, EntityId) + 'static) -> OneShot {
    create_full(|_, _, _| false, on_target_erased, true)
}

/// `create(stopAttackingWhen)`; the predicate gets `(level, body, target)` (vanilla lambdas capture the body).
pub fn create_when(stop_attacking_when: impl FnMut(&ServerLevel, &dyn Entity, &dyn Entity) -> bool + 'static) -> OneShot {
    create_full(stop_attacking_when, |_, _, _| {}, true)
}

/// `create()`.
pub fn create() -> OneShot {
    create_full(|_, _, _| false, |_, _, _| {}, true)
}

/// `create(stopAttackingWhen, onTargetErased, canGrowTiredOfTryingToReachTarget)`; `on_target_erased` gets the id of the
/// erased target (it may no longer resolve in the level).
pub fn create_full(
    mut stop_attacking_when: impl FnMut(&ServerLevel, &dyn Entity, &dyn Entity) -> bool + 'static,
    mut on_target_erased: impl FnMut(&mut ServerLevel, &mut dyn Entity, EntityId) + 'static,
    can_grow_tired_of_trying_to_reach_target: bool,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let attack_target = i.present(&ATTACK_TARGET);
    let cant_reach_since = i.registered(&CANT_REACH_WALK_TARGET_SINCE);
    i.create(move |c| {
        let target = c.get(&attack_target).0;
        let tired = c.try_get(&cant_reach_since).is_some_and(|&since| c.level.game_time() - since > TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE);
        let valid = c.level.get_entity(target).is_some_and(|t| {
            c.body.can_attack(&*c.level, t)
                && (!can_grow_tired_of_trying_to_reach_target || !tired)
                && t.is_alive()
                && !stop_attacking_when(&*c.level, &*c.body, t)
        });
        if valid {
            true
        } else {
            on_target_erased(&mut *c.level, &mut *c.body, target);
            c.erase(&attack_target);
            true
        }
    })
}
