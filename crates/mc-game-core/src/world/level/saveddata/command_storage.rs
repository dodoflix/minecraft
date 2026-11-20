//! `CommandStorage` (`data/<namespace>/command_storage.dat`, one file per namespace).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use mc_core::ident::{Identifier, is_valid_path};
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::SavedDataStorage;
use parking_lot::Mutex;

use super::{SavedDataManager, SavedDataType};
use crate::impl_saved_data;
use crate::util::codec::{self, DataResult, DecodeResult, map_decoded};

/// `CommandStorage.Container`.
#[derive(Debug, Default)]
pub struct Container {
    dirty: bool,
    storage: HashMap<String, CompoundTag>,
}

impl_saved_data!(Container);

impl Container {
    pub fn get(&self, id: &str) -> CompoundTag {
        self.storage.get(id).cloned().unwrap_or_default()
    }

    pub fn put(&mut self, id: &str, contents: CompoundTag) {
        use super::SavedData;
        if contents.is_empty() {
            self.storage.remove(id);
        } else {
            self.storage.insert(id.to_string(), contents);
        }
        self.set_dirty();
    }

    pub fn get_keys(&self, namespace: &str) -> impl Iterator<Item = Identifier> + '_ {
        let namespace = namespace.to_string();
        self.storage
            .keys()
            .filter_map(move |p| Identifier::new(&namespace, p).ok())
    }

    fn decode(tag: &Tag) -> DecodeResult<Container> {
        let m = codec::compound(tag)?;
        let entries = codec::unbounded_map(
            codec::field(m, "contents")?,
            |k| {
                if is_valid_path(k) {
                    Ok(k.to_string())
                } else {
                    Err(format!(
                        "Invalid string to use as a resource path element: {k}"
                    ))
                }
            },
            |v| codec::compound(v).cloned(),
        );
        map_decoded(entries, |entries| Container {
            dirty: false,
            storage: entries.into_iter().collect(),
        })
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut contents = CompoundTag::new();
        for (k, v) in &self.storage {
            contents.put(k.clone(), v.clone());
        }
        Ok(Tag::Compound(CompoundTag::new().with("contents", contents)))
    }

    /// `Container.type(namespace)`.
    pub fn type_(namespace: &str) -> Result<SavedDataType<Container>, String> {
        Ok(SavedDataType::new(
            Identifier::new(namespace, "command_storage").map_err(|e| e.to_string())?,
            Container::default,
            Container::decode,
            Container::encode,
            mc_datafix::DataFixTypes::SAVED_DATA_COMMAND_STORAGE,
        ))
    }
}

/// Read access to the contents for code that cannot borrow the server (the loot number and NBT providers of a running
/// level). [`CommandStorage::set`] writes through to it; a namespace nothing wrote to reads the file the saved data
/// manager would load.
#[derive(Clone, Default)]
pub struct CommandStorageView {
    state: Arc<Mutex<ViewState>>,
}

#[derive(Default)]
struct ViewState {
    namespaces: HashMap<String, HashMap<String, CompoundTag>>,
    /// Namespaces whose file the view read: like vanilla's `getContainer`, a read makes their ids suggestible.
    read_from_disk: Vec<String>,
    disk: Option<SavedDataStorage>,
}

impl std::fmt::Debug for CommandStorageView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CommandStorageView")
    }
}

impl ViewState {
    fn namespace(&mut self, namespace: &str) -> &mut HashMap<String, CompoundTag> {
        let (disk, read_from_disk) = (&self.disk, &mut self.read_from_disk);
        self.namespaces
            .entry(namespace.to_owned())
            .or_insert_with(|| {
                let file = Container::type_(namespace).ok().and_then(|ty| {
                    match disk.as_ref()?.read_data(&ty.id, ty.data_fix_type) {
                        Ok(tag) => tag,
                        Err(e) => {
                            log::error!("Error loading saved data: {ty}: {e}");
                            None
                        }
                    }
                });
                if file.is_some() {
                    read_from_disk.push(namespace.to_owned());
                }
                // like the manager, keep what a partly invalid file still decodes
                file.and_then(|tag| match Container::decode(&tag) {
                    Ok(container) => Some(container),
                    Err(partial) => partial.value,
                })
                .map(|container| container.storage)
                .unwrap_or_default()
            })
    }
}

impl CommandStorageView {
    /// `CommandStorage.get(id)`.
    pub fn get(&self, id: &Identifier) -> CompoundTag {
        let mut state = self.state.lock();
        state
            .namespace(id.namespace())
            .get(id.path())
            .cloned()
            .unwrap_or_default()
    }

    fn namespaces_read_from_disk(&self) -> Vec<String> {
        self.state.lock().read_from_disk.clone()
    }

    fn put(&self, id: &Identifier, contents: &CompoundTag) {
        let mut state = self.state.lock();
        let namespace = state.namespace(id.namespace());
        if contents.is_empty() {
            namespace.remove(id.path());
        } else {
            namespace.insert(id.path().to_owned(), contents.clone());
        }
    }
}

/// `CommandStorage`: `/data ... storage <id>` contents, namespaced per file.
#[derive(Debug, Default)]
pub struct CommandStorage {
    /// Namespaces whose container has been seen (vanilla's `namespaces` map); the containers live in the manager.
    namespaces: Vec<String>,
    view: CommandStorageView,
}

impl CommandStorage {
    pub fn new() -> Self {
        CommandStorage::default()
    }

    /// A storage whose [`CommandStorageView`] reads the namespace files of `data_folder`.
    pub fn with_data_folder(data_folder: impl Into<PathBuf>) -> Self {
        CommandStorage {
            namespaces: Vec::new(),
            view: CommandStorageView {
                state: Arc::new(Mutex::new(ViewState {
                    disk: Some(SavedDataStorage::new(data_folder)),
                    ..ViewState::default()
                })),
            },
        }
    }

    /// The shared read access (see [`CommandStorageView`]).
    pub fn view(&self) -> CommandStorageView {
        self.view.clone()
    }

    /// `get`: empty compound when the namespace has no data.
    pub fn get(&mut self, data: &mut SavedDataManager, id: &Identifier) -> CompoundTag {
        match self.get_container(data, id.namespace()) {
            Some(container) => container.get(id.path()),
            None => CompoundTag::new(),
        }
    }

    fn get_container<'a>(
        &mut self,
        data: &'a mut SavedDataManager,
        namespace: &str,
    ) -> Option<&'a mut Container> {
        let ty = Container::type_(namespace).ok()?;
        let container = data.get(&ty)?;
        if !self.namespaces.iter().any(|n| n == namespace) {
            self.namespaces.push(namespace.to_string());
        }
        Some(container)
    }

    /// `set`.
    pub fn set(&mut self, data: &mut SavedDataManager, id: &Identifier, contents: CompoundTag) {
        let Ok(ty) = Container::type_(id.namespace()) else {
            return;
        };
        if !self.namespaces.iter().any(|n| n == id.namespace()) {
            self.namespaces.push(id.namespace().to_string());
        }
        self.view.put(id, &contents);
        data.compute_if_absent(&ty).put(id.path(), contents);
    }

    /// `keys`.
    pub fn keys(&mut self, data: &mut SavedDataManager) -> Vec<Identifier> {
        for ns in self.view.namespaces_read_from_disk() {
            if !self.namespaces.contains(&ns) {
                self.namespaces.push(ns);
            }
        }
        let mut out = Vec::new();
        for ns in self.namespaces.clone() {
            if let Some(c) = self.get_container(data, &ns) {
                out.extend(c.get_keys(&ns));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use mc_world::level_storage::SavedDataStorage;

    use super::*;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    #[test]
    fn set_get_persist_and_empty_removes() {
        let dir = tempfile::tempdir().unwrap();
        let mut data = SavedDataManager::new(SavedDataStorage::new(dir.path().join("data")));
        let mut cs = CommandStorage::new();
        assert!(
            cs.get(&mut data, &id("ns:a")).is_empty(),
            "missing namespace reads as empty"
        );
        cs.set(&mut data, &id("ns:a"), CompoundTag::new().with("x", 1i32));
        cs.set(&mut data, &id("ns:b/c"), CompoundTag::new().with("y", "s"));
        assert_eq!(cs.get(&mut data, &id("ns:a")).get_int("x"), Some(1));
        data.save_and_join();
        assert!(dir.path().join("data/ns/command_storage.dat").is_file());
        // a fresh manager reads the file back
        let mut data2 = SavedDataManager::new(SavedDataStorage::new(dir.path().join("data")));
        let mut cs2 = CommandStorage::new();
        assert_eq!(
            cs2.get(&mut data2, &id("ns:b/c")).get_string("y"),
            Some("s")
        );
        let mut keys: Vec<_> = cs2
            .keys(&mut data2)
            .iter()
            .map(ToString::to_string)
            .collect();
        keys.sort();
        assert_eq!(keys, ["ns:a", "ns:b/c"]);
        cs2.set(&mut data2, &id("ns:a"), CompoundTag::new());
        assert_eq!(cs2.keys(&mut data2).len(), 1);
    }

    #[test]
    fn view_follows_writes_and_reads_unwritten_namespaces_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("data");
        let mut data = SavedDataManager::new(SavedDataStorage::new(&folder));
        let mut cs = CommandStorage::with_data_folder(&folder);
        cs.set(&mut data, &id("ns:a"), CompoundTag::new().with("x", 1i32));
        data.save_and_join();
        let view = cs.view();
        assert_eq!(view.get(&id("ns:a")).get_int("x"), Some(1));
        cs.set(&mut data, &id("ns:a"), CompoundTag::new().with("x", 2i32));
        assert_eq!(view.get(&id("ns:a")).get_int("x"), Some(2));
        cs.set(&mut data, &id("ns:a"), CompoundTag::new());
        assert!(view.get(&id("ns:a")).is_empty());
        // a storage that never wrote to the namespace reads what the file holds
        let mut data2 = SavedDataManager::new(SavedDataStorage::new(&folder));
        let mut other = CommandStorage::with_data_folder(&folder);
        other.set(&mut data2, &id("ns:b"), CompoundTag::new().with("y", 3i32));
        data2.save_and_join();
        let mut data3 = SavedDataManager::new(SavedDataStorage::new(&folder));
        let mut fresh_storage = CommandStorage::with_data_folder(&folder);
        let fresh = fresh_storage.view();
        assert_eq!(fresh.get(&id("ns:b")).get_int("y"), Some(3));
        assert!(fresh.get(&id("other:b")).is_empty());
        // reading a namespace file makes its ids suggestible, an absent file does not
        let mut keys: Vec<_> = fresh_storage
            .keys(&mut data3)
            .iter()
            .map(ToString::to_string)
            .collect();
        keys.sort();
        assert_eq!(keys, ["ns:a", "ns:b"]);
    }
}
