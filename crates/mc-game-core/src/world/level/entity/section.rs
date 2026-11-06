//! `net.minecraft.world.level.entity.EntitySection` (stores entity ids; the entities live in the manager).
use super::Visibility;
use crate::world::entity::EntityId;

/// Entities of one 16x16x16 section in insertion order (`ClassInstanceMultiMap.allInstances`). Class-filtered
/// queries of vanilla iterate a per-class subsequence of this list, so filtering this list gives the same order.
pub struct EntitySection {
    storage: Vec<EntityId>,
    chunk_status: Visibility,
}

impl EntitySection {
    pub fn new(chunk_status: Visibility) -> Self {
        Self {
            storage: Vec::new(),
            chunk_status,
        }
    }

    pub fn add(&mut self, entity: EntityId) {
        self.storage.push(entity);
    }

    /// `remove(entity)`: false if it wasn't there.
    pub fn remove(&mut self, entity: EntityId) -> bool {
        match self.storage.iter().position(|&e| e == entity) {
            Some(i) => {
                self.storage.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn entities(&self) -> &[EntityId] {
        &self.storage
    }

    pub fn status(&self) -> Visibility {
        self.chunk_status
    }

    /// Returns the previous status.
    pub fn update_chunk_status(&mut self, chunk_status: Visibility) -> Visibility {
        std::mem::replace(&mut self.chunk_status, chunk_status)
    }

    pub fn size(&self) -> usize {
        self.storage.len()
    }
}
