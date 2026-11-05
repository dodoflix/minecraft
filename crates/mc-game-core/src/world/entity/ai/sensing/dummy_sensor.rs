//! `DummySensor`.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::entity::Entity;

pub struct DummySensor;

impl Sensor for DummySensor {
    fn do_tick(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity) {}

    fn requires(&self) -> Vec<MemoryTypeId> {
        Vec::new()
    }
}
