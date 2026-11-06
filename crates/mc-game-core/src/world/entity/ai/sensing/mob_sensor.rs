//! `MobSensor<T>`: sets a boolean memory (with expiry) while a matching mob is nearby.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::NEAREST_LIVING_ENTITIES;
use crate::world::entity::ai::memory::{MemoryModuleType, MemoryTypeId};
use crate::world::entity::entity::Entity;

pub struct MobSensor {
    mob_test: fn(&dyn Entity, &dyn Entity) -> bool,
    ready_test: fn(&dyn Entity) -> bool,
    to_set: MemoryModuleType<bool>,
    memory_time_to_live: i64,
    scan_rate: i32,
}

impl MobSensor {
    /// `new MobSensor(scanRate, mobTest, readyTest, toSet, memoryTimeToLive)`.
    pub const fn new(
        scan_rate: i32,
        mob_test: fn(&dyn Entity, &dyn Entity) -> bool,
        ready_test: fn(&dyn Entity) -> bool,
        to_set: MemoryModuleType<bool>,
        memory_time_to_live: i64,
    ) -> Self {
        Self {
            mob_test,
            ready_test,
            to_set,
            memory_time_to_live,
            scan_rate,
        }
    }

    /// `checkForMobsNearby(body)`.
    pub fn check_for_mobs_nearby(&self, level: &ServerLevel, body: &mut dyn Entity) {
        let Some(entities) = body.brain().get_memory(&NEAREST_LIVING_ENTITIES) else {
            return;
        };
        let body_ref: &dyn Entity = &*body;
        let present = entities.iter().any(|r| {
            level
                .get_entity(r.0)
                .is_some_and(|e| (self.mob_test)(body_ref, e))
        });
        if present {
            self.mob_detected(body);
        }
    }

    /// `mobDetected(body)`.
    pub fn mob_detected(&self, body: &mut dyn Entity) {
        body.brain_mut()
            .set_memory_with_expiry(&self.to_set, true, self.memory_time_to_live);
    }

    /// `clearMemory(body)`.
    pub fn clear_memory(&self, body: &mut dyn Entity) {
        body.brain_mut().erase_memory(&self.to_set);
    }
}

impl Sensor for MobSensor {
    fn scan_rate(&self) -> i32 {
        self.scan_rate
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        if !(self.ready_test)(&*body) {
            self.clear_memory(body);
        } else {
            self.check_for_mobs_nearby(level, body);
        }
    }

    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![NEAREST_LIVING_ENTITIES.id()]
    }
}
