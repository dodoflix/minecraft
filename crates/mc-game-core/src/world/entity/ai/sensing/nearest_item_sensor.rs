//! `NearestItemSensor`: the closest visible item entity the mob wants to pick up.

use super::distance_to_sqr;
use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::NEAREST_VISIBLE_WANTED_ITEM;
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId};
use crate::world::entity::entity::Entity;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::mob::ext::has_line_of_sight;
use mc_data::EntityType;

/// `NearestItemSensor.MAX_DISTANCE_TO_WANTED_ITEM`.
pub const MAX_DISTANCE_TO_WANTED_ITEM: i32 = 32;

pub struct NearestItemSensor;

impl Sensor for NearestItemSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![NEAREST_VISIBLE_WANTED_ITEM.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let bb = body.get_bounding_box().inflate(32.0, 16.0, 32.0);
        let mut items = level.get_entities_of_class(&bb, |e| e.get_type() == EntityType::ITEM, |_| true);
        let body_ref: &dyn Entity = &*body;
        let distance = |id: &i32| level.get_entity(*id).map_or(f64::NAN, |e| distance_to_sqr(body_ref, e));
        items.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        let nearest = items.into_iter().find(|&id| {
            let Some(item) = level.get_entity(id) else { return false };
            let Some(stack) = item.as_any().downcast_ref::<ItemEntity>().map(ItemEntity::get_item) else { return false };
            body_ref.wants_to_pick_up(level, &stack) && super::closer_than(item, body_ref, 32.0) && has_line_of_sight(level, body_ref, item)
        });
        body.brain_mut().set_memory_opt(&NEAREST_VISIBLE_WANTED_ITEM, nearest.map(EntityRef));
    }
}
