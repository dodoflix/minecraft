//! `SetRaidStatus`.

use super::BehaviorBuilder;
use super::OneShot;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::schedule::Activity;
use crate::world::level::LevelAccessor as _;

pub fn create() -> OneShot {
    BehaviorBuilder::point(
        |level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64| {
            if level.random().next_int_bounded(20) != 0 {
                return false;
            }
            let raid_state = level
                .get_raid_at(body.block_position())
                .map(|raid| raid.has_first_wave_spawned() && !raid.is_between_waves());
            if let Some(in_wave) = raid_state {
                let brain = body.brain_mut();
                if in_wave {
                    brain.set_default_activity(Activity::RAID);
                    brain.set_active_activity_if_possible(Activity::RAID);
                } else {
                    brain.set_default_activity(Activity::PRE_RAID);
                    brain.set_active_activity_if_possible(Activity::PRE_RAID);
                }
            }
            true
        },
    )
}
