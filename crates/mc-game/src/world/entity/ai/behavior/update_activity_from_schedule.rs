//! `UpdateActivityFromSchedule`.

use super::declarative::BehaviorBuilder;
use super::OneShot;
use crate::server::level::ServerLevel;
use crate::world::entity::entity::Entity;
use crate::world::entity::schedule::Activity;

/// `body.getBrain().updateActivityFromSchedule(level.environmentAttributes(), level.getGameTime(), body.position())`.
pub fn update(level: &ServerLevel, body: &mut dyn Entity) {
    let pos = body.position();
    body.brain_mut().update_activity_from_schedule(level.game_time(), |attribute| {
        Activity::by_id(&level.attribute_activity(attribute, &pos)).expect("registered activity")
    });
}

/// `create()`.
pub fn create() -> OneShot {
    BehaviorBuilder::point(|level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64| {
        update(level, body);
        true
    })
}
