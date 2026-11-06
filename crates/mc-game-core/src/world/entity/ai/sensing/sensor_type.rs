//! `SensorType<U>` and the sensor registry (`BuiltInRegistries.SENSOR_TYPE`, declaration order).

use super::adult_sensor::AdultSensor;
use super::dummy_sensor::DummySensor;
use super::golem_sensor::GolemSensor;
use super::hurt_by_sensor::HurtBySensor;
use super::is_in_water_sensor::IsInWaterSensor;
use super::mob_sensor::MobSensor;
use super::mob_sensors::{
    AxolotlAttackablesSensor, BreezeAttackEntitySensor, FrogAttackablesSensor, WardenEntitySensor,
};
use super::nearest_bed_sensor::NearestBedSensor;
use super::nearest_item_sensor::NearestItemSensor;
use super::nearest_living_entity_sensor::NearestLivingEntitySensor;
use super::nearest_visible_living_entity_sensor::NearestVisibleLivingEntitySensor;
use super::player_sensor::PlayerSensor;
use super::repellent_sensors::{
    HoglinSpecificSensor, PiglinBruteSpecificSensor, PiglinSpecificSensor,
};
use super::secondary_poi_sensor::SecondaryPoiSensor;
use super::sensor::Sensor;
use super::tempting_sensor::TemptingSensor;
use super::villager_babies_sensor::VillagerBabiesSensor;
use super::villager_hostiles_sensor::VillagerHostilesSensor;
use crate::world::entity::ai::memory::memory_module_type::DANGER_DETECTED_RECENTLY;

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
pub static NEAREST_ITEMS: SensorType =
    SensorType::new("nearest_items", || Box::new(NearestItemSensor));
pub static NEAREST_LIVING_ENTITIES: SensorType = SensorType::new("nearest_living_entities", || {
    Box::new(NearestLivingEntitySensor)
});
pub static NEAREST_PLAYERS: SensorType =
    SensorType::new("nearest_players", || Box::new(PlayerSensor));
pub static NEAREST_BED: SensorType =
    SensorType::new("nearest_bed", || Box::new(NearestBedSensor::new()));
pub static HURT_BY: SensorType = SensorType::new("hurt_by", || Box::new(HurtBySensor));
pub static VILLAGER_HOSTILES: SensorType = SensorType::new("villager_hostiles", || {
    Box::new(NearestVisibleLivingEntitySensor(VillagerHostilesSensor))
});
pub static VILLAGER_BABIES: SensorType =
    SensorType::new("villager_babies", || Box::new(VillagerBabiesSensor));
pub static SECONDARY_POIS: SensorType =
    SensorType::new("secondary_pois", || Box::new(SecondaryPoiSensor));
pub static GOLEM_DETECTED: SensorType =
    SensorType::new("golem_detected", || Box::new(GolemSensor::new()));
pub static ARMADILLO_SCARE_DETECTED: SensorType =
    SensorType::new("armadillo_scare_detected", || {
        Box::new(MobSensor::new(
            5,
            |body, entity| body.is_scared_by(entity),
            |body| body.can_stay_rolled_up(),
            DANGER_DETECTED_RECENTLY,
            80,
        ))
    });
pub static PIGLIN_SPECIFIC_SENSOR: SensorType =
    SensorType::new("piglin_specific_sensor", || Box::new(PiglinSpecificSensor));
pub static PIGLIN_BRUTE_SPECIFIC_SENSOR: SensorType =
    SensorType::new("piglin_brute_specific_sensor", || {
        Box::new(PiglinBruteSpecificSensor)
    });
pub static HOGLIN_SPECIFIC_SENSOR: SensorType =
    SensorType::new("hoglin_specific_sensor", || Box::new(HoglinSpecificSensor));
pub static NEAREST_ADULT: SensorType =
    SensorType::new("nearest_adult", || Box::new(AdultSensor::new()));
pub static NEAREST_ADULT_ANY_TYPE: SensorType = SensorType::new("nearest_adult_any_type", || {
    Box::new(AdultSensor::any_type())
});
pub static AXOLOTL_ATTACKABLES: SensorType = SensorType::new("axolotl_attackables", || {
    Box::new(NearestVisibleLivingEntitySensor(AxolotlAttackablesSensor))
});
pub static FOOD_TEMPTATIONS: SensorType =
    SensorType::new(
        "food_temptations",
        || Box::new(TemptingSensor::for_animal()),
    );
pub static FROG_TEMPTATIONS: SensorType =
    SensorType::new("frog_temptations", || Box::new(TemptingSensor::frog()));
pub static NAUTILUS_TEMPTATIONS: SensorType = SensorType::new("nautilus_temptations", || {
    Box::new(TemptingSensor::nautilus())
});
pub static FROG_ATTACKABLES: SensorType = SensorType::new("frog_attackables", || {
    Box::new(NearestVisibleLivingEntitySensor(FrogAttackablesSensor))
});
pub static IS_IN_WATER: SensorType = SensorType::new("is_in_water", || Box::new(IsInWaterSensor));
pub static WARDEN_ENTITY_SENSOR: SensorType =
    SensorType::new("warden_entity_sensor", || Box::new(WardenEntitySensor));
pub static BREEZE_ATTACK_ENTITY_SENSOR: SensorType =
    SensorType::new("breeze_attack_entity_sensor", || {
        Box::new(BreezeAttackEntitySensor)
    });

/// The registry in declaration order .
pub static SENSOR_TYPES: &[&SensorType] = &[
    &DUMMY,
    &NEAREST_ITEMS,
    &NEAREST_LIVING_ENTITIES,
    &NEAREST_PLAYERS,
    &NEAREST_BED,
    &HURT_BY,
    &VILLAGER_HOSTILES,
    &VILLAGER_BABIES,
    &SECONDARY_POIS,
    &GOLEM_DETECTED,
    &ARMADILLO_SCARE_DETECTED,
    &PIGLIN_SPECIFIC_SENSOR,
    &PIGLIN_BRUTE_SPECIFIC_SENSOR,
    &HOGLIN_SPECIFIC_SENSOR,
    &NEAREST_ADULT,
    &NEAREST_ADULT_ANY_TYPE,
    &AXOLOTL_ATTACKABLES,
    &FOOD_TEMPTATIONS,
    &FROG_TEMPTATIONS,
    &NAUTILUS_TEMPTATIONS,
    &FROG_ATTACKABLES,
    &IS_IN_WATER,
    &WARDEN_ENTITY_SENSOR,
    &BREEZE_ATTACK_ENTITY_SENSOR,
];
