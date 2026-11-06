//! `SleepInBed`: go to sleep in the remembered bed (resting villagers).

use super::interact_with_door;
use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::memory_module_type::{
    CANT_REACH_WALK_TARGET_SINCE, DOORS_TO_CLOSE, HOME, LAST_SLEPT, LAST_WOKEN,
    NEAREST_LIVING_ENTITIES, WALK_TARGET,
};
use crate::world::entity::schedule::Activity;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_data::props;

pub const COOLDOWN_AFTER_BEING_WOKEN: i64 = 100;

pub struct SleepInBed {
    next_ok_start_time: i64,
}

impl SleepInBed {
    pub fn new() -> BehaviorBase<SleepInBed> {
        BehaviorBase::with_duration(
            entry_condition![HOME => ValuePresent, LAST_WOKEN => Registered, LAST_SLEPT => Registered, WALK_TARGET => Registered, CANT_REACH_WALK_TARGET_SINCE => Registered],
            DEFAULT_DURATION,
            DEFAULT_DURATION,
            SleepInBed {
                next_ok_start_time: 0,
            },
        )
    }
}

impl Behavior for SleepInBed {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        if body.base().is_passenger() {
            return false;
        }
        let brain = body.brain();
        let Some(target) = brain.get_memory(&HOME).cloned() else {
            return false;
        };
        if level.dimension != target.dimension {
            return false;
        }
        if let Some(last_woken) = brain.get_memory(&LAST_WOKEN) {
            let time_since_last_woken = level.game_time() - *last_woken;
            if time_since_last_woken > 0 && time_since_last_woken < COOLDOWN_AFTER_BEING_WOKEN {
                return false;
            }
        }
        let state = level.get_block_state(target.pos);
        target.pos.closer_to_center_than(&body.position(), 2.0)
            && state.is_tag("villagers_can_sleep_on_bed")
            && !state.get(props::OCCUPIED)
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        let Some(memory) = body.brain().get_memory(&HOME) else {
            return false;
        };
        let bed_pos = memory.pos;
        body.brain().is_active(Activity::REST)
            && body.base().y() > f64::from(bed_pos.y()) + 0.4
            && bed_pos.closer_to_center_than(&body.position(), 1.14)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if timestamp > self.next_ok_start_time {
            if body.brain().has_memory_value(DOORS_TO_CLOSE.id()) {
                let nearest_entities =
                    if body.brain().has_memory_value(NEAREST_LIVING_ENTITIES.id()) {
                        body.brain().get_memory(&NEAREST_LIVING_ENTITIES).cloned()
                    } else {
                        None
                    };
                interact_with_door::close_doors_in_memory(level, body, nearest_entities.as_deref());
            }
            let home = body.brain().get_memory(&HOME).map(|h| h.pos);
            if let Some(home) = home
                && body.start_sleeping(level, home)
            {
                body.brain_mut().set_memory(&LAST_SLEPT, timestamp);
            }
            body.brain_mut().erase_memory(&WALK_TARGET);
            body.brain_mut().erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
        }
    }

    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if body.is_sleeping() {
            body.stop_sleeping(level);
            self.next_ok_start_time = timestamp + 40;
        }
    }
}
