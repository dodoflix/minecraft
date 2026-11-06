//! `VillagerHostilesSensor`: the closest visible hostile mob that is within its type's acceptable distance.

use super::distance_to_sqr;
use super::nearest_visible_living_entity_sensor::NearestVisibleLivingEntityMatcher;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::NEAREST_HOSTILE;
use crate::world::entity::ai::memory::{EntityRef, MemoryModuleType};
use crate::world::entity::entity::Entity;
use mc_data::EntityType;

/// `ACCEPTABLE_DISTANCE_FROM_HOSTILES`.
fn acceptable_distance_from_hostiles(entity_type: EntityType) -> Option<f32> {
    Some(match entity_type {
        EntityType::DROWNED => 8.0,
        EntityType::EVOKER => 12.0,
        EntityType::HUSK => 8.0,
        EntityType::ILLUSIONER => 12.0,
        EntityType::PILLAGER => 15.0,
        EntityType::RAVAGER => 12.0,
        EntityType::VEX => 8.0,
        EntityType::VINDICATOR => 10.0,
        EntityType::ZOGLIN => 10.0,
        EntityType::ZOMBIE => 8.0,
        EntityType::ZOMBIE_VILLAGER => 8.0,
        _ => return None,
    })
}

pub struct VillagerHostilesSensor;

impl NearestVisibleLivingEntityMatcher for VillagerHostilesSensor {
    fn is_matching_entity(
        &self,
        _level: &ServerLevel,
        body: &dyn Entity,
        mob: &dyn Entity,
    ) -> bool {
        // `isHostile(mob) && isClose(body, mob)`
        match acceptable_distance_from_hostiles(mob.get_type()) {
            Some(threshold) => distance_to_sqr(mob, body) <= f64::from(threshold * threshold),
            None => false,
        }
    }

    fn get_memory_to_set(&self) -> MemoryModuleType<EntityRef> {
        NEAREST_HOSTILE
    }
}
