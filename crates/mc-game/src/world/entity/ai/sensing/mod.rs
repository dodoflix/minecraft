//! `net.minecraft.world.entity.ai.sensing`: sensors (the brain's input side) and their registry.
//!
//! A sensor is a [`sensor::Sensor`] (`do_tick`, `requires`, `scan_rate`), created by a [`sensor_type::SensorType`] and
//! driven by [`crate::world::entity::ai::brain::Brain::tick`]: the countdown (`timeToTick`, initially
//! `random.nextInt(scanRate)` from the owner's random) is decremented every tick and the sensor scans when it hits 0
//! (`tick` also refreshes the shared targeting range from the body's `FOLLOW_RANGE`).
//!
//! Mob-specific sensors (`Piglin*`, `Hoglin*`, `Warden`, `Breeze`, `Frog`, `Armadillo`, `Villager`) are implemented
//! here against the `Entity` trait queries (entity type, `is_baby`, ...); where vanilla calls a method of the mob
//! class itself the `Entity` trait has the vanilla-named method with the vanilla default, which the mob overrides.

pub mod adult_sensor;
pub mod dummy_sensor;
pub mod golem_sensor;
pub mod hurt_by_sensor;
pub mod is_in_water_sensor;
pub mod mob_sensor;
pub mod mob_sensors;
pub mod nearest_item_sensor;
pub mod nearest_living_entity_sensor;
pub mod nearest_visible_living_entity_sensor;
pub mod player_sensor;
pub mod repellent_sensors;
pub mod sensing;
pub mod sensor;
pub mod sensor_type;
pub mod tempting_sensor;
pub mod villager_babies_sensor;
pub mod villager_hostiles_sensor;

pub use sensing::Sensing;
pub use sensor::{Sensor, SensorInstance};
pub use sensor_type::SensorType;

use crate::world::entity::entity::Entity;

/// `body.distanceToSqr(other)`.
pub fn distance_to_sqr(body: &dyn Entity, other: &dyn Entity) -> f64 {
    body.base().distance_to_sqr_vec(other.position())
}

/// `a.closerThan(b, distance)`.
pub fn closer_than(a: &dyn Entity, b: &dyn Entity, distance: f64) -> bool {
    let (pa, pb) = (a.position(), b.position());
    let (dx, dy, dz) = (pb.x - pa.x, pb.y - pa.y, pb.z - pa.z);
    dx * dx + dy * dy + dz * dz < distance * distance
}
