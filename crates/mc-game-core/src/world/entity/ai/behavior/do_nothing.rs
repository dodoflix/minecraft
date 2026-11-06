//! `DoNothing`: runs (does nothing) for a random duration.

use super::{BehaviorControl, Status};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::entity::Entity;
use mc_core::random::RandomSource;

pub struct DoNothing {
    min_duration: i32,
    max_duration: i32,
    status: Status,
    end_timestamp: i64,
}

impl DoNothing {
    pub fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            min_duration,
            max_duration,
            status: Status::Stopped,
            end_timestamp: 0,
        }
    }
}

impl BehaviorControl for DoNothing {
    fn get_status(&self) -> Status {
        self.status
    }

    fn get_required_memories(&self) -> Vec<MemoryTypeId> {
        Vec::new()
    }

    fn try_start(
        &mut self,
        level: &mut ServerLevel,
        _body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        self.status = Status::Running;
        let bound = self
            .max_duration
            .wrapping_add(1)
            .wrapping_sub(self.min_duration);
        let duration = self
            .min_duration
            .wrapping_add(level.tick_state.random.next_int_bounded(bound));
        self.end_timestamp = timestamp + i64::from(duration);
        true
    }

    fn tick_or_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if timestamp > self.end_timestamp {
            self.do_stop(level, body, timestamp);
        }
    }

    fn do_stop(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {
        self.status = Status::Stopped;
    }

    fn debug_string(&self) -> String {
        "DoNothing".to_string()
    }
}
