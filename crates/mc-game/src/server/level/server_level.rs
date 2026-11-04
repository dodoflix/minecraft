//! `ServerLevel`. Skeleton: the LevelAgent slice grows this struct (chunk source, tickets, entity storage, ticks).
use crate::world::entity::EntityId;
use crate::world::level::DimensionKey;
use crate::world::level::entity::PersistentEntitySectionManager;

pub struct ServerLevel {
    pub dimension: DimensionKey,
    /// `entityManager` (+ `entityTickList`): owns every entity of the level (see `server_level_entities.rs`).
    pub entity_manager: PersistentEntitySectionManager,
    /// `players`: ids of the players in this level, in join order.
    pub players: Vec<EntityId>,
}

impl ServerLevel {
    pub fn new(dimension: DimensionKey) -> Self {
        Self { dimension, entity_manager: PersistentEntitySectionManager::new(None), players: Vec::new() }
    }
}
