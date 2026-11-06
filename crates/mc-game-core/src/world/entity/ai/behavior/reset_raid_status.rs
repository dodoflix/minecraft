//! `ResetRaidStatus`.

use super::BehaviorBuilder;
use super::OneShot;
use super::village_util::update_activity_from_schedule;
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
            let reset = level
                .get_raid_at(body.block_position())
                .is_none_or(|raid| raid.is_stopped() || raid.is_loss());
            if reset {
                body.brain_mut().set_default_activity(Activity::IDLE);
                update_activity_from_schedule(level, body);
            }
            true
        },
    )
}
