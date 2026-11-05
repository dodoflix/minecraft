//! `SavedData`, `SavedDataType` and `SavedDataStorage` (typed, cached, dirty-tracked).

use std::any::Any;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::{SavedDataStorage, create_directories_safe, write_compressed};

use crate::util::codec::{DataResult, DecodeResult};

/// `SavedData`: an object with a dirty flag that the storage persists. Implementors keep the flag themselves
/// (vanilla's base class field) because their mutators call `setDirty()`; use [`impl_saved_data!`] for the plumbing.
pub trait SavedData: Any + Send {
    fn is_dirty(&self) -> bool;
    /// `setDirty(boolean)`.
    fn set_dirty_to(&mut self, dirty: bool);
    /// `setDirty()`.
    fn set_dirty(&mut self) {
        self.set_dirty_to(true);
    }
}

/// Implements [`SavedData`] for a struct with a `dirty: bool` field.
#[macro_export]
macro_rules! impl_saved_data {
    ($ty:ty) => {
        impl $crate::world::level::saveddata::SavedData for $ty {
            fn is_dirty(&self) -> bool {
                self.dirty
            }
            fn set_dirty_to(&mut self, dirty: bool) {
                self.dirty = dirty;
            }
        }
    };
}

/// `SavedDataType<T>`: file id (`data/<namespace>/<path>.dat`), constructor and the NBT codec of the `data` tag.
/// Datafix types are not modelled (no DFU port): files are read as-is.
///
/// The functions are closures so that types whose codec needs context (enabled features, registries) can capture it.
pub struct SavedDataType<T: SavedData> {
    pub id: Identifier,
    pub constructor: Arc<dyn Fn() -> T + Send + Sync>,
    pub decode: Arc<dyn Fn(&Tag) -> DecodeResult<T> + Send + Sync>,
    pub encode: Arc<dyn Fn(&T) -> DataResult<Tag> + Send + Sync>,
}

impl<T: SavedData> SavedDataType<T> {
    pub fn new(
        id: Identifier,
        constructor: impl Fn() -> T + Send + Sync + 'static,
        decode: impl Fn(&Tag) -> DecodeResult<T> + Send + Sync + 'static,
        encode: impl Fn(&T) -> DataResult<Tag> + Send + Sync + 'static,
    ) -> Self {
        SavedDataType { id, constructor: Arc::new(constructor), decode: Arc::new(decode), encode: Arc::new(encode) }
    }
}

impl<T: SavedData> Clone for SavedDataType<T> {
    fn clone(&self) -> Self {
        SavedDataType { id: self.id.clone(), constructor: self.constructor.clone(), decode: self.decode.clone(), encode: self.encode.clone() }
    }
}

impl<T: SavedData> std::fmt::Display for SavedDataType<T> {
    /// `SavedDataType.toString`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SavedDataType[{}]", self.id)
    }
}

type ErasedEncode = Box<dyn Fn(&dyn SavedData) -> DataResult<Tag> + Send>;

struct Entry {
    /// `Optional<SavedData>`: `None` caches a failed/absent read.
    data: Option<Box<dyn SavedData>>,
    encode: ErasedEncode,
}

/// Encoded dirty files collected by [`SavedDataManager::schedule_save`]; writing them is IO only, so it can run on
/// any thread while the game thread keeps ticking.
#[derive(Debug)]
pub struct PendingSave {
    writes: Vec<(PathBuf, CompoundTag)>,
}

impl PendingSave {
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    /// `tryWrite` for each file: errors are logged, not propagated (like vanilla).
    pub fn write(self) {
        for (path, tag) in self.writes {
            let result: io::Result<()> = (|| {
                if let Some(parent) = path.parent() {
                    create_directories_safe(parent)?;
                }
                write_compressed(&tag, &path)
            })();
            if let Err(e) = result {
                log::error!("Could not save data to {}: {e}", path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()));
            }
        }
    }
}

/// `SavedDataStorage`: one per `data/` folder (the server's and one per dimension).
pub struct SavedDataManager {
    storage: SavedDataStorage,
    cache: HashMap<Identifier, Entry>,
    closed: bool,
}

impl SavedDataManager {
    pub fn new(storage: SavedDataStorage) -> Self {
        SavedDataManager { storage, cache: HashMap::new(), closed: false }
    }

    pub fn storage(&self) -> &SavedDataStorage {
        &self.storage
    }

    /// `computeIfAbsent`.
    pub fn compute_if_absent<T: SavedData>(&mut self, ty: &SavedDataType<T>) -> &mut T {
        if self.get(ty).is_none() {
            self.set(ty, (ty.constructor)());
        }
        self.get(ty).expect("just inserted")
    }

    /// Shared access to an instance that was already loaded (`get`/`compute_if_absent`/`set` ran before); `None` when it
    /// was never loaded. For `&self` readers (level ticking) of data the server loaded at startup.
    pub fn peek<T: SavedData>(&self, ty: &SavedDataType<T>) -> Option<&T> {
        let entry = self.cache.get(&ty.id)?;
        entry.data.as_ref().and_then(|d| (&**d as &dyn Any).downcast_ref::<T>())
    }

    /// `get`: reads and caches on first access; `None` when absent or unreadable (logged).
    pub fn get<T: SavedData>(&mut self, ty: &SavedDataType<T>) -> Option<&mut T> {
        if !self.cache.contains_key(&ty.id) {
            let data = self.read_saved_data(ty).map(|d| Box::new(d) as Box<dyn SavedData>);
            self.cache.insert(ty.id.clone(), Entry { data, encode: erased_encoder(ty) });
        }
        let entry = self.cache.get_mut(&ty.id)?;
        entry.data.as_mut().and_then(|d| (&mut **d as &mut dyn Any).downcast_mut::<T>())
    }

    /// `readSavedData`.
    fn read_saved_data<T: SavedData>(&self, ty: &SavedDataType<T>) -> Option<T> {
        let tag = self.storage.get(&ty.id)?;
        match (ty.decode)(&tag) {
            Ok(data) => Some(data),
            // `resultOrPartial(logger)`: log, keep the partial value if there is one.
            Err(partial) => {
                log::error!("Failed to parse saved data for '{ty}': {}", partial.error);
                partial.value
            }
        }
    }

    /// `set`: stores `data` and marks it dirty. Returns the stored instance.
    pub fn set<T: SavedData>(&mut self, ty: &SavedDataType<T>, mut data: T) -> &mut T {
        data.set_dirty();
        self.cache.insert(ty.id.clone(), Entry { data: Some(Box::new(data)), encode: erased_encoder(ty) });
        self.get(ty).expect("just inserted")
    }

    /// `collectDirtyTagsToSave` + the file paths: encodes every dirty instance and clears its flag.
    /// Encoding failures are errors in vanilla (`getOrThrow`); here they are logged and the file is skipped.
    pub fn schedule_save(&mut self) -> PendingSave {
        assert!(!self.closed, "Trying to schedule save when SavedDataStorage is already closed");
        let mut writes = Vec::new();
        for (id, entry) in &mut self.cache {
            let Some(data) = entry.data.as_mut().filter(|d| d.is_dirty()) else { continue };
            let tag = match (entry.encode)(&**data) {
                Ok(tag) => tag,
                Err(e) => {
                    log::error!("Failed to encode saved data '{id}': {e}");
                    continue;
                }
            };
            data.set_dirty_to(false);
            match self.storage.get_data_file(id) {
                Ok(path) => writes.push((path, SavedDataStorage::encode_file(tag))),
                Err(e) => log::error!("Could not save data {id}: {e}"),
            }
        }
        PendingSave { writes }
    }

    /// `saveAndJoin`.
    pub fn save_and_join(&mut self) {
        self.schedule_save().write();
    }

    /// `close`: saves and refuses further saves.
    pub fn close(&mut self) {
        assert!(!self.closed, "Trying to close SavedDataStorage when it is already closed");
        self.save_and_join();
        self.closed = true;
    }
}

fn erased_encoder<T: SavedData>(ty: &SavedDataType<T>) -> ErasedEncode {
    let encode = ty.encode.clone();
    Box::new(move |data| match (data as &dyn Any).downcast_ref::<T>() {
        Some(data) => encode(data),
        None => Err("saved data type mismatch".to_string()),
    })
}
