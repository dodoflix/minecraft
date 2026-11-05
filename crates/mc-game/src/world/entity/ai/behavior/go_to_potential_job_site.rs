//! `GoToPotentialJobSite`.

use super::behavior_utils::set_walk_and_look_target_memories_pos;
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::memory_module_type::POTENTIAL_JOB_SITE;
use crate::world::entity::schedule::Activity;

const TICKS_UNTIL_TIMEOUT: i32 = 1200;

pub struct GoToPotentialJobSite {
    speed_modifier: f32,
}

impl GoToPotentialJobSite {
    pub fn new(speed_modifier: f32) -> BehaviorBase<GoToPotentialJobSite> {
        BehaviorBase::with_timeout(entry_condition![POTENTIAL_JOB_SITE => ValuePresent], TICKS_UNTIL_TIMEOUT, GoToPotentialJobSite { speed_modifier })
    }
}

impl Behavior for GoToPotentialJobSite {
    fn check_extra_start_conditions(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        body.brain().get_active_non_core_activity().is_none_or(|a| a == Activity::IDLE || a == Activity::WORK || a == Activity::PLAY)
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        body.brain().has_memory_value(POTENTIAL_JOB_SITE.id())
    }

    fn tick(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(pos) = body.brain().get_memory(&POTENTIAL_JOB_SITE).map(|g| g.pos) else { return };
        set_walk_and_look_target_memories_pos(body, pos, self.speed_modifier, 1);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(global) = body.brain().get_memory(&POTENTIAL_JOB_SITE).cloned() {
            // `server.getLevel(globalPos.dimension())`: only the level the villager is in is reachable from here
            if global.dimension == level.dimension {
                let manager = level.poi_manager();
                if manager.exists(global.pos, &|_| true) {
                    let _ = manager.release(global.pos);
                }
            }
        }
        body.brain_mut().erase_memory(&POTENTIAL_JOB_SITE);
    }
}
