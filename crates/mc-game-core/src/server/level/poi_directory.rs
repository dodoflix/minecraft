//! `server.getLevel(dimension).getPoiManager()` for code that runs inside one level: a villager reads and releases the
//! point of interest it remembers in another level (`AssignProfessionFromJobSite`, `GoToPotentialJobSite`,
//! `Villager.releasePoi`) while its own tick runs, so the level of that point cannot be taken out of the server for it.
//! Every level registers the handle of its POI manager here when the server creates it.

use crate::world::entity::ai::village::poi::PoiManager;
use crate::world::level::DimensionKey;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct PoiDirectory(Arc<RwLock<HashMap<DimensionKey, PoiManager>>>);

impl PoiDirectory {
    pub fn register(&self, dimension: DimensionKey, manager: PoiManager) {
        self.0.write().insert(dimension, manager);
    }

    pub fn get(&self, dimension: &DimensionKey) -> Option<PoiManager> {
        self.0.read().get(dimension).cloned()
    }
}
