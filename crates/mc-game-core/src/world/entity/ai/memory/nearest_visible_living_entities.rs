//! `NearestVisibleLivingEntities`: the nearby living entities (sorted by distance) filtered lazily by
//! `Sensor.isEntityTargetable`, the result per entity cached until the sensor replaces the memory.

use super::value::EntityRef;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::ai::sensing::sensor;
use crate::world::entity::entity::Entity;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct NearestVisibleLivingEntities {
    nearby: Arc<Vec<EntityRef>>,
    /// `Object2BooleanOpenHashMap` of `lineOfSightTest`; shared by clones of the memory value.
    cache: Arc<Mutex<HashMap<EntityId, bool>>>,
}

/// Identity of the Java object: two values are equal when they share their cache.
impl PartialEq for NearestVisibleLivingEntities {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cache, &other.cache)
    }
}

impl NearestVisibleLivingEntities {
    /// `NearestVisibleLivingEntities.empty()`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// `new NearestVisibleLivingEntities(level, body, livingEntities)`.
    pub fn new(living_entities: Vec<EntityRef>) -> Self {
        Self { nearby: Arc::new(living_entities), cache: Arc::default() }
    }

    /// `nearbyEntities()`.
    pub fn nearby_entities(&self) -> &[EntityRef] {
        &self.nearby
    }

    /// `lineOfSightTest`: `cache.computeIfAbsent(entity, e -> Sensor.isEntityTargetable(level, body, e))`.
    fn line_of_sight(&self, level: &ServerLevel, body: &dyn Entity, entity: &dyn Entity) -> bool {
        if let Some(&cached) = self.cache.lock().get(&entity.id()) {
            return cached;
        }
        // evaluated outside the lock: the test may read other brain memories
        let result = sensor::is_entity_targetable(level, body, entity);
        self.cache.lock().insert(entity.id(), result);
        result
    }

    /// `findClosest(filter)`.
    pub fn find_closest(&self, level: &ServerLevel, body: &dyn Entity, mut filter: impl FnMut(&dyn Entity) -> bool) -> Option<EntityRef> {
        for &nearby in self.nearby.iter() {
            if let Some(e) = level.get_entity(nearby.0)
                && filter(e)
                && self.line_of_sight(level, body, e)
            {
                return Some(nearby);
            }
        }
        None
    }

    /// `findAll(filter)` / `find(filter)`.
    pub fn find_all(&self, level: &ServerLevel, body: &dyn Entity, mut filter: impl FnMut(&dyn Entity) -> bool) -> Vec<EntityRef> {
        self.nearby
            .iter()
            .copied()
            .filter(|nearby| level.get_entity(nearby.0).is_some_and(|e| filter(e) && self.line_of_sight(level, body, e)))
            .collect()
    }

    /// `contains(LivingEntity)`.
    pub fn contains(&self, level: &ServerLevel, body: &dyn Entity, target: EntityId) -> bool {
        self.nearby.contains(&EntityRef(target)) && level.get_entity(target).is_some_and(|e| self.line_of_sight(level, body, e))
    }

    /// `contains(LivingEntity)` for a target that is out of the level (the running entity): the entity itself is given.
    pub fn contains_entity(&self, level: &ServerLevel, body: &dyn Entity, target: &dyn Entity) -> bool {
        self.nearby.contains(&EntityRef(target.id())) && self.line_of_sight(level, body, target)
    }

    /// `contains(Predicate)`.
    pub fn contains_matching(&self, level: &ServerLevel, body: &dyn Entity, filter: impl FnMut(&dyn Entity) -> bool) -> bool {
        self.find_closest(level, body, filter).is_some()
    }
}
