//! `IsInWaterSensor`.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::IS_IN_WATER;
use crate::world::entity::ai::memory::{MemoryTypeId, Unit};
use crate::world::entity::entity::Entity;

pub struct IsInWaterSensor;

impl Sensor for IsInWaterSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![IS_IN_WATER.id()]
    }

    fn do_tick(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity) {
        if body.is_in_water() {
            body.brain_mut().set_memory(&IS_IN_WATER, Unit);
        } else {
            body.brain_mut().erase_memory(&IS_IN_WATER);
        }
    }
}
