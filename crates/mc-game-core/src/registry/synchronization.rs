//! `core.RegistrySynchronization` + `tags.TagNetworkSerialization` as plain data for the network layer: what the
//! configuration phase sends (`registry_data`, `update_tags`).

use std::collections::HashSet;

use mc_core::ident::Identifier;
use mc_core::java_collections::{hash_map_ordered, HASH_MAP_DEFAULT_CAPACITY};
use mc_nbt::Tag;

use super::access::{LayeredRegistryAccess, RegistryAccess};
use super::codec::json_to_nbt;
use super::keys::SYNCHRONIZED_REGISTRIES;
use crate::server::packs::KnownPack;
use crate::server::registry_layer::RegistryLayer;

/// `RegistrySynchronization.PackedRegistryEntry`: `data` is `None` when the client knows the entry from a known pack.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedRegistryEntry {
    pub id: Identifier,
    pub data: Option<Tag>,
}

/// `isNetworkable(key)`.
pub fn is_networkable(registry: &str) -> bool {
    SYNCHRONIZED_REGISTRIES.contains(&registry)
}

/// `packRegistries(ops, registries, clientKnownPacks, output)`: one `(registry key, entries)` per synchronized registry,
/// in `SYNCHRONIZED_REGISTRIES` order. Entries that come from a pack the client knows carry no data.
pub fn pack_registries(registries: &RegistryAccess, client_known_packs: &HashSet<KnownPack>) -> Vec<(String, Vec<PackedRegistryEntry>)> {
    let mut out = Vec::new();
    for key in SYNCHRONIZED_REGISTRIES {
        let Some(registry) = registries.registry(key) else { continue };
        let mut packed = Vec::with_capacity(registry.len());
        for entry in registry.entries() {
            let can_skip = entry.info.known_pack.as_ref().is_some_and(|k| client_known_packs.contains(k));
            let data = if can_skip {
                None
            } else {
                // the typed network codec's encoding, else the raw JSON converted (see `DecodedEntry::network`)
                Some(entry.network.clone().unwrap_or_else(|| json_to_nbt(&entry.json)))
            };
            packed.push(PackedRegistryEntry { id: entry.id.clone(), data });
        }
        out.push(((*key).to_owned(), packed));
    }
    out
}

/// `TagNetworkSerialization.NetworkPayload`: tag id -> entry protocol ids.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NetworkPayload {
    pub tags: Vec<(Identifier, Vec<u32>)>,
}

impl NetworkPayload {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }
}

/// `serializeTagsToNetwork(registries)`: the tags of the networkable dynamic registries and of every static registry
/// (registries without tags are left out). Registries ordered by key (vanilla: identity-hash order of the `ResourceKey`s,
/// random per run), tags in `HashMap<Identifier, _>` iteration order.
pub fn serialize_tags_to_network(registries: &LayeredRegistryAccess<RegistryLayer>) -> Vec<(String, NetworkPayload)> {
    let networked = registries.get_access_from(RegistryLayer::World);
    let mut keys: Vec<String> = networked.registries().filter(|r| is_networkable(r.key())).map(|r| r.key().to_owned()).collect();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut collect = |access: &RegistryAccess, key: &str, result: &mut Vec<(String, NetworkPayload)>| {
        if let Some(registry) = access.registry(key) {
            let mut tags: Vec<(Identifier, Vec<u32>)> = registry.tags().map(|(id, ids)| (id.clone(), ids.to_vec())).collect();
            // `Map<Identifier, IntList>` is a `HashMap`: bucket order of the identifier hash. Vanilla fills it from an
            // `IdentityHashMap<TagKey>` (random per run), so ties inside a bucket are random there; ours: by identifier.
            tags.sort_by(|a, b| a.0.cmp(&b.0));
            let tags = hash_map_ordered(tags, |(id, _)| id.java_hash_code(), HASH_MAP_DEFAULT_CAPACITY);
            if !tags.is_empty() && seen.insert(key.to_owned()) {
                result.push((key.to_owned(), NetworkPayload { tags }));
            }
        }
    };
    keys.sort();
    for key in &keys {
        collect(&networked, key, &mut result);
    }
    let statics = registries.get_layer(RegistryLayer::Static);
    for registry in statics.registries() {
        collect(statics, registry.key(), &mut result);
    }
    result
}
