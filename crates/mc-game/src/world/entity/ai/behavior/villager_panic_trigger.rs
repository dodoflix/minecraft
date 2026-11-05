//! `VillagerPanicTrigger`: switch to the panic activity when hurt or a hostile is near.

use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::memory_module_type::{BREED_TARGET, HURT_BY, INTERACTION_TARGET, LOOK_TARGET, NEAREST_HOSTILE, PATH, WALK_TARGET};
use crate::world::entity::npc::villager::villager_mut;
use crate::world::entity::schedule::Activity;

pub struct VillagerPanicTrigger;

impl VillagerPanicTrigger {
    pub fn new() -> BehaviorBase<VillagerPanicTrigger> {
        BehaviorBase::with_duration(Vec::new(), DEFAULT_DURATION, DEFAULT_DURATION, VillagerPanicTrigger)
    }
}

/// `hasHostile(myBody)`.
pub fn has_hostile(my_body: &dyn Entity) -> bool {
    my_body.brain().has_memory_value(NEAREST_HOSTILE.id())
}

/// `isHurt(myBody)`.
pub fn is_hurt(my_body: &dyn Entity) -> bool {
    my_body.brain().has_memory_value(HURT_BY.id())
}

impl Behavior for VillagerPanicTrigger {
    fn can_still_use(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        is_hurt(body) || has_hostile(body)
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if is_hurt(body) || has_hostile(body) {
            let brain = body.brain_mut();
            if !brain.is_active(Activity::PANIC) {
                brain.erase_memory(&PATH);
                brain.erase_memory(&WALK_TARGET);
                brain.erase_memory(&LOOK_TARGET);
                brain.erase_memory(&BREED_TARGET);
                brain.erase_memory(&INTERACTION_TARGET);
            }
            brain.set_active_activity_if_possible(Activity::PANIC);
        }
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if timestamp % 100 == 0 {
            villager_mut(body).spawn_golem_if_needed(level, timestamp, 3);
        }
    }
}
