//! `SetRoarTarget`.

use super::warden;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::EntityRef;
use crate::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE, ROAR_TARGET};
use crate::world::entity::monster::warden::Warden;

/// `SetRoarTarget.create(targetFinderFunction)`.
pub fn create(target_finder_function: fn(&Warden, &ServerLevel) -> Option<EntityId>) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let roar_target = i.absent(&ROAR_TARGET);
    let _attack_target = i.absent(&ATTACK_TARGET);
    let cant_reach_since = i.registered(&CANT_REACH_WALK_TARGET_SINCE);
    i.create(move |c| {
        let body = warden(&*c.body);
        let target = target_finder_function(body, &*c.level).filter(|id| c.level.get_entity(*id).is_some_and(|e| body.can_target_entity_ref(&*c.level, e)));
        let Some(target) = target else { return false };
        c.set(&roar_target, EntityRef(target));
        c.erase(&cant_reach_since);
        true
    })
}
