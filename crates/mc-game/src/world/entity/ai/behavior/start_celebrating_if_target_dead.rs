//! `StartCelebratingIfTargetDead`.

use super::declarative::BehaviorBuilder;
use super::OneShot;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{ANGRY_AT, ATTACK_TARGET, CELEBRATE_LOCATION, DANCING};
use crate::world::entity::entity::Entity;

/// `create(celebrateDuration, dancePredicate)`: the predicate gets `(level, body, target)` (vanilla lambdas read the level through the body).
pub fn create(celebrate_duration: i32, dance_predicate: impl Fn(&ServerLevel, &dyn Entity, &dyn Entity) -> bool + 'static) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let attack_target = i.present(&ATTACK_TARGET);
    let angry_at = i.registered(&ANGRY_AT);
    let celebrate_at = i.absent(&CELEBRATE_LOCATION);
    let dancing = i.registered(&DANCING);
    i.create(move |c| {
        let Some(target) = c.level.get_entity(c.get(&attack_target).0) else { return false };
        if !target.is_dead_or_dying() {
            return false;
        }
        let dance = dance_predicate(&*c.level, &*c.body, target);
        let target_pos = target.block_position();
        let forget = !target.is_player() || c.level.forgive_dead_players();
        if dance {
            c.set_with_expiry(&dancing, true, i64::from(celebrate_duration));
        }
        c.set_with_expiry(&celebrate_at, target_pos, i64::from(celebrate_duration));
        if forget {
            c.erase(&attack_target);
            c.erase(&angry_at);
        }
        true
    })
}
