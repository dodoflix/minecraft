//! `NearestLivingEntitySensor<T>`: `NEAREST_LIVING_ENTITIES` + `NEAREST_VISIBLE_LIVING_ENTITIES`.

use super::distance_to_sqr;
use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{
    NEAREST_LIVING_ENTITIES, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId, NearestVisibleLivingEntities};
use crate::world::entity::entity::Entity;
use mc_data::Attribute;

#[derive(Default)]
pub struct NearestLivingEntitySensor;

/// `doTick` of `NearestLivingEntitySensor` (shared with `WardenEntitySensor` / `BreezeAttackEntitySensor` which call `super.doTick`).
pub fn scan_nearest_living_entities(level: &mut ServerLevel, body: &mut dyn Entity) {
    let follow_range = body
        .living()
        .map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE));
    let bounding_box = body
        .get_bounding_box()
        .inflate(follow_range, follow_range, follow_range);
    let body_id = body.id();
    let mut living: Vec<EntityRef> = level
        .get_entities_of_class(
            &bounding_box,
            |e| e.living().is_some(),
            |mob| mob.id() != body_id && mob.is_alive(),
        )
        .into_iter()
        .map(EntityRef)
        .collect();
    let body_ref: &dyn Entity = &*body;
    let distance = |r: &EntityRef| {
        level
            .get_entity(r.0)
            .map_or(f64::NAN, |e| distance_to_sqr(body_ref, e))
    };
    // `Comparator.comparingDouble(body::distanceToSqr)` on the (stable) `List.sort`
    living.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
    let brain = body.brain_mut();
    brain.set_memory(&NEAREST_LIVING_ENTITIES, living.clone());
    brain.set_memory(
        &NEAREST_VISIBLE_LIVING_ENTITIES,
        NearestVisibleLivingEntities::new(living),
    );
}

impl Sensor for NearestLivingEntitySensor {
    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        scan_nearest_living_entities(level, body);
    }

    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![
            NEAREST_LIVING_ENTITIES.id(),
            NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        ]
    }
}
