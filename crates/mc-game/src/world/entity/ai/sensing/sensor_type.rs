//! `SensorType<U>` and the sensor registry (`BuiltInRegistries.SENSOR_TYPE`, declaration order).

use super::adult_sensor::AdultSensor;
use super::dummy_sensor::DummySensor;
use super::golem_sensor::GolemSensor;
use super::hurt_by_sensor::HurtBySensor;
use super::is_in_water_sensor::IsInWaterSensor;
use super::nearest_living_entity_sensor::NearestLivingEntitySensor;
use super::nearest_visible_living_entity_sensor::NearestVisibleLivingEntitySensor;
use super::player_sensor::PlayerSensor;
use super::sensor::Sensor;
use super::villager_babies_sensor::VillagerBabiesSensor;
use super::villager_hostiles_sensor::VillagerHostilesSensor;

/// A registered sensor type: a name and a factory (`create()`).
pub struct SensorType {
    pub name: &'static str,
    factory: fn() -> Box<dyn Sensor>,
}

impl SensorType {
    const fn new(name: &'static str, factory: fn() -> Box<dyn Sensor>) -> Self {
        Self { name, factory }
    }

    /// `create()`.
    pub fn create(&self) -> Box<dyn Sensor> {
        (self.factory)()
    }

    /// `BuiltInRegistries.SENSOR_TYPE.getValue(id)`.
    pub fn by_name(name: &str) -> Option<&'static SensorType> {
        SENSOR_TYPES.iter().copied().find(|t| t.name == name)
    }
}

impl std::fmt::Debug for SensorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SensorType(minecraft:{})", self.name)
    }
}

pub static DUMMY: SensorType = SensorType::new("dummy", || Box::new(DummySensor));
pub static NEAREST_LIVING_ENTITIES: SensorType = SensorType::new("nearest_living_entities", || Box::new(NearestLivingEntitySensor));
pub static NEAREST_PLAYERS: SensorType = SensorType::new("nearest_players", || Box::new(PlayerSensor));
pub static HURT_BY: SensorType = SensorType::new("hurt_by", || Box::new(HurtBySensor));
pub static VILLAGER_HOSTILES: SensorType = SensorType::new("villager_hostiles", || Box::new(NearestVisibleLivingEntitySensor(VillagerHostilesSensor)));
pub static VILLAGER_BABIES: SensorType = SensorType::new("villager_babies", || Box::new(VillagerBabiesSensor));
pub static GOLEM_DETECTED: SensorType = SensorType::new("golem_detected", || Box::new(GolemSensor::new()));
pub static NEAREST_ADULT: SensorType = SensorType::new("nearest_adult", || Box::new(AdultSensor::new()));
pub static NEAREST_ADULT_ANY_TYPE: SensorType = SensorType::new("nearest_adult_any_type", || Box::new(AdultSensor::any_type()));
pub static IS_IN_WATER: SensorType = SensorType::new("is_in_water", || Box::new(IsInWaterSensor));

/// The registry in declaration order.
pub static SENSOR_TYPES: &[&SensorType] = &[
    &DUMMY,
    &NEAREST_LIVING_ENTITIES,
    &NEAREST_PLAYERS,
    &HURT_BY,
    &VILLAGER_HOSTILES,
    &VILLAGER_BABIES,
    &GOLEM_DETECTED,
    &NEAREST_ADULT,
    &NEAREST_ADULT_ANY_TYPE,
    &IS_IN_WATER,
];
