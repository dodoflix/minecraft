//! `StartAttacking`.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::memory::EntityRef;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE};
use mc_game_core::world::entity::entity::Entity;

/// `create(targetFinderFunction)`.
pub fn create(target_finder: impl FnMut(&ServerLevel, &dyn Entity) -> Option<EntityId> + 'static) -> OneShot {
    create_with(|_, _| true, target_finder)
}

/// `create(canAttackPredicate, targetFinderFunction)`.
pub fn create_with(
    mut can_attack_predicate: impl FnMut(&ServerLevel, &dyn Entity) -> bool + 'static,
    mut target_finder: impl FnMut(&ServerLevel, &dyn Entity) -> Option<EntityId> + 'static,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let attack_target = i.absent(&ATTACK_TARGET);
    let cant_reach_since = i.registered(&CANT_REACH_WALK_TARGET_SINCE);
    i.create(move |c| {
        if !can_attack_predicate(&*c.level, &*c.body) {
            return false;
        }
        let Some(target) = target_finder(&*c.level, &*c.body) else { return false };
        let can_attack = c.level.get_entity(target).is_some_and(|t| c.body.can_attack(&*c.level, t));
        if !can_attack {
            return false;
        }
        c.set(&attack_target, EntityRef(target));
        c.erase(&cant_reach_since);
        true
    })
}
