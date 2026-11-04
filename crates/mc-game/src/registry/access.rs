//! `Registry` / `MappedRegistry`, `RegistryAccess` and `LayeredRegistryAccess`.
//!
//! A [`Registry`] is either one of the code-defined *static* registries (`mc_data`: ids and names are generated data, tags come
//! from the selected data packs) or a *dynamic* one loaded from data packs (entries keep their raw JSON, and the typed value
//! when a decoder for that registry is linked in, see [`super::decoders`]). Entry ids follow vanilla's registration order.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use serde_json::Value;

use crate::server::packs::KnownPack;

/// `RegistrationInfo`: where an entry came from (`knownPackInfo` lets the network layer skip its data).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RegistrationInfo {
    pub known_pack: Option<KnownPack>,
    /// `Lifecycle.stable()` for vanilla-pack entries, `experimental` for everything else.
    pub stable: bool,
}

/// An entry of a dynamic registry.
pub struct RegistryEntry {
    pub id: Identifier,
    /// The JSON file content as read from the pack.
    pub json: Arc<Value>,
    /// The typed value, if a decoder for the registry exists.
    pub value: Option<Arc<dyn Any + Send + Sync>>,
    /// The network-codec NBT of the entry (`registry_data` payload), when the typed decoder produced one.
    pub network: Option<mc_nbt::Tag>,
    /// The id of the pack the winning file came from (for error messages).
    pub source_pack: String,
    pub info: RegistrationInfo,
}

pub(crate) struct DynamicStore {
    pub(crate) entries: Vec<RegistryEntry>,
    pub(crate) by_id: HashMap<Identifier, u32>,
}

#[derive(Clone)]
enum Store {
    Static(&'static mc_data::registry::Registry),
    Dynamic(Arc<DynamicStore>),
}

/// A registry with its resolved tags (cheap to clone: dynamic entries are shared).
#[derive(Clone)]
pub struct Registry {
    key: String,
    store: Store,
    /// tag id -> entry ids in tag order (`TagLoader` keeps the first-seen order of the tag's expanded entries).
    tags: HashMap<Identifier, Arc<[u32]>>,
}

impl Registry {
    /// A registry backed by `mc_data` (no tags yet).
    pub fn from_static(registry: &'static mc_data::registry::Registry) -> Registry {
        Registry { key: registry.name().to_owned(), store: Store::Static(registry), tags: HashMap::new() }
    }

    pub(crate) fn dynamic(key: &str, entries: Vec<RegistryEntry>) -> Registry {
        let by_id = entries.iter().enumerate().map(|(i, e)| (e.id.clone(), i as u32)).collect();
        Registry { key: key.to_owned(), store: Store::Dynamic(Arc::new(DynamicStore { entries, by_id })), tags: HashMap::new() }
    }

    /// Same registry with its tags replaced (`Registry.prepareTagReload` + apply).
    pub fn with_tags(self, tags: HashMap<Identifier, Arc<[u32]>>) -> Registry {
        Registry { tags, ..self }
    }

    /// Registry key, e.g. `minecraft:worldgen/biome`.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self.store, Store::Dynamic(_))
    }

    pub fn len(&self) -> usize {
        match &self.store {
            Store::Static(r) => r.len(),
            Store::Dynamic(d) => d.entries.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `getId(key)`: the protocol id of an entry.
    pub fn id_of(&self, id: &Identifier) -> Option<u32> {
        match &self.store {
            Store::Static(r) => r.id_of(&id.to_string()),
            Store::Dynamic(d) => d.by_id.get(id).copied(),
        }
    }

    pub fn contains(&self, id: &Identifier) -> bool {
        self.id_of(id).is_some()
    }

    /// `byId(id)` as the entry's identifier.
    pub fn name_of(&self, id: u32) -> Option<Identifier> {
        match &self.store {
            Store::Static(r) => r.name_of(id).and_then(Identifier::try_parse),
            Store::Dynamic(d) => d.entries.get(id as usize).map(|e| e.id.clone()),
        }
    }

    /// All entry names in id order.
    pub fn names(&self) -> Vec<Identifier> {
        (0..self.len() as u32).filter_map(|i| self.name_of(i)).collect()
    }

    /// The raw entry of a dynamic registry.
    pub fn entry(&self, id: u32) -> Option<&RegistryEntry> {
        match &self.store {
            Store::Dynamic(d) => d.entries.get(id as usize),
            Store::Static(_) => None,
        }
    }

    pub fn entry_by_key(&self, id: &Identifier) -> Option<&RegistryEntry> {
        self.entry(self.id_of(id)?)
    }

    /// Entries of a dynamic registry in id order.
    pub fn entries(&self) -> &[RegistryEntry] {
        match &self.store {
            Store::Dynamic(d) => &d.entries,
            Store::Static(_) => &[],
        }
    }

    /// The typed value of an entry (`None` if the registry has no typed decoder for `T` or the entry does not exist).
    pub fn get<T: Any + Send + Sync>(&self, id: &Identifier) -> Option<&T> {
        self.entry_by_key(id)?.value.as_ref()?.downcast_ref::<T>()
    }

    pub fn get_by_id<T: Any + Send + Sync>(&self, id: u32) -> Option<&T> {
        self.entry(id)?.value.as_ref()?.downcast_ref::<T>()
    }

    /// `registrationInfo(key)` of a dynamic registry entry.
    pub fn registration_info(&self, id: &Identifier) -> Option<&RegistrationInfo> {
        self.entry_by_key(id).map(|e| &e.info)
    }

    /// The entry ids of tag `tag`, in the order vanilla's `TagLoader` expanded them.
    pub fn tag(&self, tag: &Identifier) -> Option<&[u32]> {
        self.tags.get(tag).map(|t| &t[..])
    }

    pub fn tags(&self) -> impl Iterator<Item = (&Identifier, &[u32])> {
        self.tags.iter().map(|(k, v)| (k, &v[..]))
    }

    pub fn tag_count(&self) -> usize {
        self.tags.len()
    }

    /// `holder.is(tag)`.
    pub fn is_in_tag(&self, id: u32, tag: &Identifier) -> bool {
        self.tag(tag).is_some_and(|t| t.contains(&id))
    }
}

/// `RegistryAccess`: a set of registries by key; cheap to clone.
#[derive(Clone, Default)]
pub struct RegistryAccess {
    registries: Arc<IndexMap<String, Arc<Registry>>>,
}

impl RegistryAccess {
    /// `RegistryAccess.EMPTY`.
    pub fn empty() -> RegistryAccess {
        RegistryAccess::default()
    }

    pub fn from_registries(registries: impl IntoIterator<Item = Registry>) -> RegistryAccess {
        RegistryAccess { registries: Arc::new(registries.into_iter().map(|r| (r.key.clone(), Arc::new(r))).collect()) }
    }

    pub(crate) fn from_arcs(registries: impl IntoIterator<Item = Arc<Registry>>) -> RegistryAccess {
        RegistryAccess { registries: Arc::new(registries.into_iter().map(|r| (r.key.clone(), r)).collect()) }
    }

    /// `lookup(registryKey)`.
    pub fn registry(&self, key: &str) -> Option<&Arc<Registry>> {
        self.registries.get(key)
    }

    /// `lookupOrThrow(registryKey)`.
    pub fn registry_or_throw(&self, key: &str) -> &Arc<Registry> {
        self.registry(key).unwrap_or_else(|| panic!("Missing registry: ResourceKey[minecraft:root / {key}]"))
    }

    /// `registries()`.
    pub fn registries(&self) -> impl Iterator<Item = &Arc<Registry>> {
        self.registries.values()
    }

    pub fn contains_registry(&self, key: &str) -> bool {
        self.registries.contains_key(key)
    }

    /// Typed value of entry `key` of dynamic registry `registry` (`registryAccess.registry(R).get(key).value()`).
    pub fn get<T: Any + Send + Sync>(&self, registry: &str, key: &Identifier) -> Option<&T> {
        self.registry(registry)?.get::<T>(key)
    }

    /// Entry ids of tag `tag` in `registry` (`registry.getTagOrEmpty`).
    pub fn tag(&self, registry: &str, tag: &Identifier) -> Option<&[u32]> {
        self.registry(registry)?.tag(tag)
    }

    /// `holder.is(tag)` for the entry `entry` of `registry`.
    pub fn is_in_tag(&self, registry: &str, entry: &Identifier, tag: &Identifier) -> bool {
        self.registry(registry).is_some_and(|r| r.id_of(entry).is_some_and(|id| r.is_in_tag(id, tag)))
    }
}

/// `LayeredRegistryAccess<T>`: registries grouped in ordered layers; later layers must not repeat a registry of an earlier one.
#[derive(Clone)]
pub struct LayeredRegistryAccess<L: Copy + PartialEq + std::fmt::Debug> {
    keys: Vec<L>,
    values: Vec<RegistryAccess>,
    composite: RegistryAccess,
}

impl<L: Copy + PartialEq + std::fmt::Debug> LayeredRegistryAccess<L> {
    pub fn new(keys: Vec<L>) -> LayeredRegistryAccess<L> {
        let values = vec![RegistryAccess::empty(); keys.len()];
        LayeredRegistryAccess::build(keys, values)
    }

    fn build(keys: Vec<L>, values: Vec<RegistryAccess>) -> LayeredRegistryAccess<L> {
        let composite = RegistryAccess::from_arcs(collect_registries(&values));
        LayeredRegistryAccess { keys, values, composite }
    }

    fn index_or_throw(&self, layer: L) -> usize {
        self.keys.iter().position(|k| *k == layer).unwrap_or_else(|| panic!("Can't find {layer:?} inside {:?}", self.keys))
    }

    pub fn get_layer(&self, layer: L) -> &RegistryAccess {
        &self.values[self.index_or_throw(layer)]
    }

    /// `getAccessForLoading(layer)`: everything before `layer`.
    pub fn get_access_for_loading(&self, layer: L) -> RegistryAccess {
        let index = self.index_or_throw(layer);
        RegistryAccess::from_arcs(collect_registries(&self.values[..index]))
    }

    /// `getAccessFrom(layer)`: `layer` and everything after it.
    pub fn get_access_from(&self, layer: L) -> RegistryAccess {
        let index = self.index_or_throw(layer);
        RegistryAccess::from_arcs(collect_registries(&self.values[index..]))
    }

    /// `replaceFrom(fromLayer, layers...)`: replaces `fromLayer` and the following layers (missing ones become empty).
    pub fn replace_from(&self, from_layer: L, layers: Vec<RegistryAccess>) -> LayeredRegistryAccess<L> {
        let index = self.index_or_throw(from_layer);
        assert!(layers.len() <= self.values.len() - index, "Too many values to replace");
        let mut new_values: Vec<RegistryAccess> = self.values[..index].to_vec();
        new_values.extend(layers);
        new_values.resize(self.values.len(), RegistryAccess::empty());
        LayeredRegistryAccess::build(self.keys.clone(), new_values)
    }

    pub fn composite_access(&self) -> &RegistryAccess {
        &self.composite
    }
}

fn collect_registries(layers: &[RegistryAccess]) -> Vec<Arc<Registry>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for access in layers {
        for r in access.registries() {
            assert!(seen.insert(r.key().to_owned()), "Duplicated registry ResourceKey[minecraft:root / {}]", r.key());
            out.push(r.clone());
        }
    }
    out
}
