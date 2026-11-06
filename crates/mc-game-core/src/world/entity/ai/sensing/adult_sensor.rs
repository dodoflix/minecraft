//! `AdultSensor` and `AdultSensorAnyType`: `NEAREST_VISIBLE_ADULT`.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::ai::memory::memory_module_type::{
    NEAREST_VISIBLE_ADULT, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use crate::world::entity::entity::Entity;
use mc_data::EntityType;

/// `AdultSensor` (same entity type) or `AdultSensorAnyType` (`FOLLOWABLE_FRIENDLY_MOBS`).
pub struct AdultSensor {
    any_type: bool,
}

impl AdultSensor {
    pub fn new() -> Self {
        Self { any_type: false }
    }

    pub fn any_type() -> Self {
        Self { any_type: true }
    }
}

impl Default for AdultSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for AdultSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![
            NEAREST_VISIBLE_ADULT.id(),
            NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let Some(visible) = body
            .brain()
            .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
            .cloned()
        else {
            return;
        };
        let body_type = body.get_type();
        let any_type = self.any_type;
        let followable = mc_data::tags::get::<EntityType>("followable_friendly_mobs");
        let adult = visible.find_closest(level, &*body, |e| {
            if any_type {
                followable
                    .as_ref()
                    .is_some_and(|t| t.contains(e.get_type()))
                    && !e.is_baby()
            } else {
                e.get_type() == body_type && !e.is_baby()
            }
        });
        body.brain_mut()
            .set_memory_opt(&NEAREST_VISIBLE_ADULT, adult);
    }
}
