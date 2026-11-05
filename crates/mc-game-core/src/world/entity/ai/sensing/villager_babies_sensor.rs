//! `VillagerBabiesSensor`: `VISIBLE_VILLAGER_BABIES`.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{NEAREST_VISIBLE_LIVING_ENTITIES, VISIBLE_VILLAGER_BABIES};
use crate::world::entity::ai::memory::{MemoryTypeId, NearestVisibleLivingEntities};
use crate::world::entity::entity::Entity;
use mc_data::EntityType;

pub struct VillagerBabiesSensor;

impl Sensor for VillagerBabiesSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![VISIBLE_VILLAGER_BABIES.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let visible = body.brain().get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES).cloned().unwrap_or_else(NearestVisibleLivingEntities::empty);
        let babies = visible.find_all(level, &*body, |e| e.get_type() == EntityType::VILLAGER && e.is_baby());
        body.brain_mut().set_memory(&VISIBLE_VILLAGER_BABIES, babies);
    }
}
