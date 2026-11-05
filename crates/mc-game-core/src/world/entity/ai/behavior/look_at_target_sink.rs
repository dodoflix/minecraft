//! `LookAtTargetSink`: worked example of a classic [`Behavior`] (see the module header of
//! [`crate::world::entity::ai::behavior`]): turns the head towards the `LOOK_TARGET` while it stays visible.

use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::LOOK_TARGET;
use crate::world::entity::entity::Entity;
use crate::world::entity::mob;

pub struct LookAtTargetSink;

impl LookAtTargetSink {
    /// `new LookAtTargetSink(minDuration, maxDuration)`: `super(ImmutableMap.of(LOOK_TARGET, VALUE_PRESENT), min, max)`.
    pub fn new(min_duration: i32, max_duration: i32) -> BehaviorBase<LookAtTargetSink> {
        BehaviorBase::with_duration(entry_condition![LOOK_TARGET => ValuePresent], min_duration, max_duration, LookAtTargetSink)
    }
}

impl Behavior for LookAtTargetSink {
    fn can_still_use(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        body.brain().get_memory(&LOOK_TARGET).is_some_and(|pos| pos.is_visible_by(level, &*body))
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&LOOK_TARGET);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(target) = body.brain().get_memory(&LOOK_TARGET) {
            let p = target.current_position(level);
            mob::look_at_xyz(body, p.x, p.y, p.z);
        }
    }
}
