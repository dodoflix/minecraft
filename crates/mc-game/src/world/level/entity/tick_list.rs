//! `net.minecraft.world.level.entity.EntityTickList`.
use crate::world::entity::EntityId;
use indexmap::IndexSet;

/// Ids of the entities that tick, in insertion order. Vanilla copies the map when it is modified during an
/// iteration so the running iteration sees the snapshot taken at its start; [`EntityTickList::snapshot`] gives
/// the same semantics (modifications during the walk don't change what the walk visits).
#[derive(Default)]
pub struct EntityTickList {
    active: IndexSet<EntityId>,
}

impl EntityTickList {
    pub fn add(&mut self, entity: EntityId) {
        self.active.insert(entity);
    }

    pub fn remove(&mut self, entity: EntityId) {
        self.active.shift_remove(&entity);
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.active.contains(&entity)
    }

    /// The ids `forEach` would visit.
    pub fn snapshot(&self) -> Vec<EntityId> {
        self.active.iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}
