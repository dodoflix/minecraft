//! The server-wide map storage: vanilla reads maps from `MinecraftServer.getDataStorage()` (the root `data/` folder)
//! from every level. Levels cannot reach the server's `saved_data`, so the map files live in their own
//! [`SavedDataManager`] on [`crate::server::shared::ServerShared`] (disjoint files: `maps/<id>.dat`, `maps/last_id.dat`).

use super::map_item_saved_data::MapItemSavedData;
use super::map_markers::MapIndex;
use crate::server::level::ServerLevel;
use crate::world::level::saveddata::SavedDataManager;
use mc_protocol::components::MapId;
use mc_world::level_storage::SavedDataStorage;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default)]
pub struct MapStorage {
    inner: Mutex<Option<SavedDataManager>>,
}

impl MapStorage {
    /// Points the storage at the server's `data/` folder (`MinecraftServer.getDataStorage()`).
    pub fn init(&self, storage: SavedDataStorage) {
        *self.inner.lock() = Some(SavedDataManager::new(storage));
    }

    /// Runs `f` over the storage. Levels created without a server (`ServerLevel::detached`) get a throwaway folder.
    /// Map code must not call back into the storage from `f` (the lock is not re-entrant).
    pub fn with<R>(&self, f: impl FnOnce(&mut SavedDataManager) -> R) -> R {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let mut guard = self
            .inner
            .try_lock()
            .expect("map saved data accessed re-entrantly");
        let manager = guard.get_or_insert_with(|| {
            let dir = std::env::temp_dir().join(format!(
                "mc-detached-maps-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            SavedDataManager::new(SavedDataStorage::new(dir))
        });
        f(manager)
    }

    /// `saveAndJoin` / the asynchronous part of `scheduleSave` of the map files.
    pub fn save(&self, flush: bool) {
        let pending = {
            let mut guard = self.inner.lock();
            let Some(manager) = guard.as_mut() else {
                return;
            };
            if flush {
                manager.save_and_join();
                return;
            }
            manager.schedule_save()
        };
        if !pending.is_empty() {
            std::thread::Builder::new()
                .name("IO-Worker".to_owned())
                .spawn(move || pending.write())
                .ok();
        }
    }

    /// `SavedDataStorage.close()`.
    pub fn close(&self) {
        if let Some(manager) = self.inner.lock().as_mut() {
            manager.close();
        }
    }
}

impl ServerLevel {
    /// `getMapData(id)`: runs `f` over the map's saved data (`None` if there is none). Do not call other map storage
    /// methods from inside `f`.
    pub fn with_map_data<R>(
        &mut self,
        id: MapId,
        f: impl FnOnce(&mut ServerLevel, &mut MapItemSavedData) -> R,
    ) -> Option<R> {
        let shared = self.shared.clone();
        shared.map_storage.with(|storage| {
            let data = storage.get(&MapItemSavedData::type_(id))?;
            Some(f(self, data))
        })
    }

    /// `getMapData(id)` for readers that only need to look at the saved data.
    pub fn read_map_data<R>(&self, id: MapId, f: impl FnOnce(&MapItemSavedData) -> R) -> Option<R> {
        self.shared.map_storage.with(|storage| {
            storage
                .get(&MapItemSavedData::type_(id))
                .map(|data| f(data))
        })
    }

    /// `setMapData(id, data)`.
    pub fn set_map_data(&self, id: MapId, data: MapItemSavedData) {
        self.shared.map_storage.with(|storage| {
            storage.set(&MapItemSavedData::type_(id), data);
        });
    }

    /// `getFreeMapId()`.
    pub fn get_free_map_id(&self) -> MapId {
        self.shared.map_storage.with(|storage| {
            storage
                .compute_if_absent(&MapIndex::type_())
                .get_next_map_id()
        })
    }
}
