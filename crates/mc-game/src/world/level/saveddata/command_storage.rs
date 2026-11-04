//! `CommandStorage` (`data/<namespace>/command_storage.dat`, one file per namespace).

use std::collections::HashMap;

use mc_core::ident::{Identifier, is_valid_path};
use mc_nbt::{CompoundTag, Tag};

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
        self.storage.keys().filter_map(move |p| Identifier::new(&namespace, p).ok())
    }

    fn decode(tag: &Tag) -> DecodeResult<Container> {
        let m = codec::compound(tag)?;
        let entries = codec::unbounded_map(
            codec::field(m, "contents")?,
            |k| if is_valid_path(k) { Ok(k.to_string()) } else { Err(format!("Invalid string to use as a resource path element: {k}")) },
            |v| codec::compound(v).cloned(),
        );
        map_decoded(entries, |entries| Container { dirty: false, storage: entries.into_iter().collect() })
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
        Ok(SavedDataType::new(Identifier::new(namespace, "command_storage").map_err(|e| e.to_string())?, Container::default, Container::decode, Container::encode))
    }
}

/// `CommandStorage`: `/data ... storage <id>` contents, namespaced per file.
#[derive(Debug, Default)]
pub struct CommandStorage {
    /// Namespaces whose container has been seen (vanilla's `namespaces` map); the containers live in the manager.
    namespaces: Vec<String>,
}

impl CommandStorage {
    pub fn new() -> Self {
        CommandStorage::default()
    }

    /// `get`: empty compound when the namespace has no data.
    pub fn get(&mut self, data: &mut SavedDataManager, id: &Identifier) -> CompoundTag {
        match self.get_container(data, id.namespace()) {
            Some(container) => container.get(id.path()),
            None => CompoundTag::new(),
        }
    }

    fn get_container<'a>(&mut self, data: &'a mut SavedDataManager, namespace: &str) -> Option<&'a mut Container> {
        let ty = Container::type_(namespace).ok()?;
        let container = data.get(&ty)?;
        if !self.namespaces.iter().any(|n| n == namespace) {
            self.namespaces.push(namespace.to_string());
        }
        Some(container)
    }

    /// `set`.
    pub fn set(&mut self, data: &mut SavedDataManager, id: &Identifier, contents: CompoundTag) {
        let Ok(ty) = Container::type_(id.namespace()) else { return };
        if !self.namespaces.iter().any(|n| n == id.namespace()) {
            self.namespaces.push(id.namespace().to_string());
        }
        data.compute_if_absent(&ty).put(id.path(), contents);
    }

    /// `keys`.
    pub fn keys(&mut self, data: &mut SavedDataManager) -> Vec<Identifier> {
        let mut out = Vec::new();
        for ns in self.namespaces.clone() {
            if let Some(c) = self.get_container(data, &ns) {
                out.extend(c.get_keys(&ns));
            }
        }
        out
    }
}
