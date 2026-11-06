//! `OneShot<E>` and `Trigger<E>`.

use super::{BehaviorControl, Status};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::entity::Entity;

/// `Trigger<E>`: `boolean trigger(level, body, timestamp)`.
pub trait Trigger: 'static {
    fn trigger(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) -> bool;
}

impl<F: FnMut(&mut ServerLevel, &mut dyn Entity, i64) -> bool + 'static> Trigger for F {
    fn trigger(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) -> bool {
        self(level, body, timestamp)
    }
}

/// `OneShot<E>`: a behavior that runs its trigger once when started and is stopped again by the next `tick_or_stop`.
pub struct OneShot {
    status: Status,
    trigger: Box<dyn Trigger>,
    required_memories: Vec<MemoryTypeId>,
    debug: String,
}

impl OneShot {
    pub fn new(trigger: impl Trigger, required_memories: Vec<MemoryTypeId>, debug: String) -> Self {
        Self {
            status: Status::Stopped,
            trigger: Box::new(trigger),
            required_memories,
            debug,
        }
    }
}

impl Trigger for OneShot {
    fn trigger(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) -> bool {
        self.trigger.trigger(level, body, timestamp)
    }
}

impl BehaviorControl for OneShot {
    fn get_status(&self) -> Status {
        self.status
    }

    fn get_required_memories(&self) -> Vec<MemoryTypeId> {
        self.required_memories.clone()
    }

    fn try_start(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        if self.trigger.trigger(level, body, timestamp) {
            self.status = Status::Running;
            true
        } else {
            false
        }
    }

    fn tick_or_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.do_stop(level, body, timestamp);
    }

    fn do_stop(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {
        self.status = Status::Stopped;
    }

    fn debug_string(&self) -> String {
        self.debug.clone()
    }
}
