//! `NearestVisibleLivingEntitySensor`: finds the closest visible entity matching a predicate and stores it.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::NEAREST_VISIBLE_LIVING_ENTITIES;
use crate::world::entity::ai::memory::{EntityRef, MemoryModuleType, MemoryTypeId};
use crate::world::entity::entity::Entity;

/// The abstract methods of vanilla `NearestVisibleLivingEntitySensor`.
pub trait NearestVisibleLivingEntityMatcher: 'static {
    /// `isMatchingEntity(level, body, mob)`.
    fn is_matching_entity(&self, level: &ServerLevel, body: &dyn Entity, mob: &dyn Entity) -> bool;
    /// `getMemoryToSet()`.
    fn get_memory_to_set(&self) -> MemoryModuleType<EntityRef>;
    /// Extra `requires()` of a subclass.
    fn extra_requires(&self) -> Vec<MemoryTypeId> {
        Vec::new()
    }
}

/// Adapts a [`NearestVisibleLivingEntityMatcher`] to a [`Sensor`].
pub struct NearestVisibleLivingEntitySensor<M: NearestVisibleLivingEntityMatcher>(pub M);

impl<M: NearestVisibleLivingEntityMatcher> Sensor for NearestVisibleLivingEntitySensor<M> {
    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let nearest = body
            .brain()
            .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
            .cloned()
            .and_then(|visible| visible.find_closest(level, &*body, |mob| self.0.is_matching_entity(level, &*body, mob)));
        body.brain_mut().set_memory_opt(&self.0.get_memory_to_set(), nearest);
    }

    fn requires(&self) -> Vec<MemoryTypeId> {
        let mut r = vec![self.0.get_memory_to_set().id(), NEAREST_VISIBLE_LIVING_ENTITIES.id()];
        r.extend(self.0.extra_requires());
        r
    }
}
