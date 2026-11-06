//! `WakeUp`.

use super::{BehaviorBuilder, OneShot};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::schedule::Activity;

pub fn create() -> OneShot {
    BehaviorBuilder::point(
        |level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64| {
            if !body.brain().is_active(Activity::REST) && body.is_sleeping() {
                body.stop_sleeping(level);
                true
            } else {
                false
            }
        },
    )
}
