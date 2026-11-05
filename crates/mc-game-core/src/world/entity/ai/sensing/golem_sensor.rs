//! `GolemSensor`: `GOLEM_DETECTED_RECENTLY` while an iron golem is among the nearby living entities.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{GOLEM_DETECTED_RECENTLY, NEAREST_LIVING_ENTITIES};
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::entity::Entity;
use mc_data::EntityType;

/// `GolemSensor.GOLEM_SCAN_RATE`.
pub const GOLEM_SCAN_RATE: i32 = 200;
/// `GolemSensor.MEMORY_TIME_TO_LIVE`.
const MEMORY_TIME_TO_LIVE: i64 = 599;

pub struct GolemSensor {
    scan_rate: i32,
}

impl GolemSensor {
    /// `new GolemSensor()`.
    pub fn new() -> Self {
        Self::with_scan_rate(GOLEM_SCAN_RATE)
    }

    /// `new GolemSensor(scanRate)`.
    pub fn with_scan_rate(scan_rate: i32) -> Self {
        Self { scan_rate }
    }
}

impl Default for GolemSensor {
    fn default() -> Self {
        Self::new()
    }
}

/// `GolemSensor.checkForNearbyGolem(body)`.
pub fn check_for_nearby_golem(level: &ServerLevel, body: &mut dyn Entity) {
    let Some(entities) = body.brain().get_memory(&NEAREST_LIVING_ENTITIES) else { return };
    let golem_present = entities.iter().any(|r| level.get_entity(r.0).is_some_and(|e| e.get_type() == EntityType::IRON_GOLEM));
    if golem_present {
        golem_detected(body);
    }
}

/// `GolemSensor.golemDetected(body)`.
pub fn golem_detected(body: &mut dyn Entity) {
    body.brain_mut().set_memory_with_expiry(&GOLEM_DETECTED_RECENTLY, true, MEMORY_TIME_TO_LIVE);
}

impl Sensor for GolemSensor {
    fn scan_rate(&self) -> i32 {
        self.scan_rate
    }

    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![NEAREST_LIVING_ENTITIES.id(), GOLEM_DETECTED_RECENTLY.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        check_for_nearby_golem(level, body);
    }
}
