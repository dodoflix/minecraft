//! `StructureSet` (+ `StructureSelectionEntry`) and the `worldgen/structure_set` registry.

use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::sync::Arc;

use serde_json::Value;

use mc_core::ident::Identifier;
use mc_world::palette::BiomeId;

use super::placement::StructurePlacement;
use crate::biome::BiomeRegistry;
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{
    Fields, LoadResult, parse_identifier, parse_list, parse_positive_int,
};

pub const STRUCTURE_SET_REGISTRY: &str = "worldgen/structure_set";
pub const STRUCTURE_REGISTRY: &str = "worldgen/structure";

/// `StructureSet.StructureSelectionEntry`. The structure is a reference to a `worldgen/structure` entry; its
/// definition is not decoded here (inline structure definitions are rejected, see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureSelectionEntry {
    pub structure: Identifier,
    pub weight: i32,
}

impl StructureSelectionEntry {
    /// `StructureSelectionEntry.CODEC`; `structure_exists` tells whether the id is a registered structure.
    fn from_json(
        value: &Value,
        structure_exists: &dyn Fn(&Identifier) -> bool,
    ) -> LoadResult<Self> {
        let fields = Fields::new(value)?;
        Ok(StructureSelectionEntry {
            structure: fields.required("structure", |v| match v {
                Value::String(_) => {
                    let id = parse_identifier(v)?;
                    if structure_exists(&id) {
                        Ok(id)
                    } else {
                        Err(LoadError::new(format!(
                            "Failed to get element {id} from registry minecraft:worldgen/structure"
                        )))
                    }
                }
                _ => Err(LoadError::new(
                    "Inline structure definitions are not supported here",
                )),
            })?,
            weight: fields.required("weight", parse_positive_int)?,
        })
    }
}

/// `StructureSet`.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureSet {
    pub structures: Vec<StructureSelectionEntry>,
    pub placement: StructurePlacement,
}

impl StructureSet {
    /// `StructureSet.DIRECT_CODEC`.
    pub fn from_json(
        value: &Value,
        biomes: &BiomeRegistry,
        structure_exists: &dyn Fn(&Identifier) -> bool,
    ) -> LoadResult<StructureSet> {
        let fields = Fields::new(value)?;
        Ok(StructureSet {
            structures: fields.required("structures", |v| {
                parse_list(v, |e| {
                    StructureSelectionEntry::from_json(e, structure_exists)
                })
            })?,
            placement: fields
                .required("placement", |v| StructurePlacement::from_json(v, biomes))?,
        })
    }
}

/// A `Holder<StructureSet>` of a registered set: its registry id and the shared value.
#[derive(Clone, Debug)]
pub struct StructureSetHolder {
    id: Identifier,
    set: Arc<StructureSet>,
}

impl StructureSetHolder {
    pub fn new(id: Identifier, set: StructureSet) -> StructureSetHolder {
        StructureSetHolder {
            id,
            set: Arc::new(set),
        }
    }

    /// The registry key.
    pub fn id(&self) -> &Identifier {
        &self.id
    }

    /// `value()`.
    pub fn value(&self) -> &StructureSet {
        &self.set
    }

    /// Holder identity (vanilla keys maps by the set/placement object).
    pub(crate) fn ptr_eq(&self, other: &StructureSetHolder) -> bool {
        Arc::ptr_eq(&self.set, &other.set)
    }
}

impl Deref for StructureSetHolder {
    type Target = StructureSet;

    fn deref(&self) -> &StructureSet {
        &self.set
    }
}

/// The `worldgen/structure_set` registry in registration order (`HolderLookup.listElements()`): identifiers
/// ordered like vanilla's resource listing (`Identifier.compareTo`: path, then namespace).
#[derive(Debug, Default)]
pub struct StructureSetRegistry {
    entries: Vec<StructureSetHolder>,
    ids: HashMap<Identifier, usize>,
    /// `Structure.biomes()` of every structure referenced by a set, when loaded through [`Self::load`].
    structure_biomes: HashMap<Identifier, Vec<BiomeId>>,
}

impl StructureSetRegistry {
    /// Loads every `worldgen/structure_set` entry of the data source. Any entry that fails to decode, names a
    /// missing structure or a missing `exclusion_zone.other_set`, or whose structure has no decodable `biomes`
    /// field fails the whole load (vanilla aborts the registry load on any element error).
    pub fn load(
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<StructureSetRegistry> {
        let mut keys = registries.list(STRUCTURE_SET_REGISTRY);
        keys.sort();
        let structure_exists = |id: &Identifier| registries.raw(STRUCTURE_REGISTRY, id).is_some();
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let json = registries
                .raw(STRUCTURE_SET_REGISTRY, &key)
                .ok_or_else(|| {
                    LoadError::new("no such registry entry").in_entry(STRUCTURE_SET_REGISTRY, &key)
                })?;
            let set = StructureSet::from_json(&json, biomes, &structure_exists)
                .map_err(|e| e.in_entry(STRUCTURE_SET_REGISTRY, &key))?;
            entries.push((key, set));
        }
        let mut registry = StructureSetRegistry::from_entries(entries)?;
        let mut seen = HashSet::new();
        for holder in &registry.entries {
            for entry in &holder.structures {
                if seen.insert(&entry.structure) {
                    let structure_biomes =
                        structure_biomes(registries, biomes, &entry.structure)
                            .map_err(|e| e.in_entry(STRUCTURE_REGISTRY, &entry.structure))?;
                    registry
                        .structure_biomes
                        .insert(entry.structure.clone(), structure_biomes);
                }
            }
        }
        Ok(registry)
    }

    /// A registry of the given entries in the given order. Fails if an `exclusion_zone.other_set` names a set
    /// that is not among them.
    pub fn from_entries(
        entries: Vec<(Identifier, StructureSet)>,
    ) -> LoadResult<StructureSetRegistry> {
        let ids: HashMap<Identifier, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (id.clone(), i))
            .collect();
        for (id, set) in &entries {
            let zone = match &set.placement {
                StructurePlacement::RandomSpread(p) => p.spreading.exclusion_zone.as_ref(),
                StructurePlacement::ConcentricRings(p) => p.spreading.exclusion_zone.as_ref(),
                StructurePlacement::DimensionOrigin => None,
            };
            if let Some(zone) = zone
                && !ids.contains_key(&zone.other_set)
            {
                return Err(LoadError::new(format!(
                    "Failed to get element {} from registry minecraft:worldgen/structure_set",
                    zone.other_set
                ))
                .at("placement")
                .in_entry(STRUCTURE_SET_REGISTRY, id));
            }
        }
        Ok(StructureSetRegistry {
            entries: entries
                .into_iter()
                .map(|(id, set)| StructureSetHolder::new(id, set))
                .collect(),
            ids,
            structure_biomes: HashMap::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `get(key)`.
    pub fn get(&self, id: &Identifier) -> Option<&StructureSetHolder> {
        self.ids.get(id).map(|&i| &self.entries[i])
    }

    /// `listElements()` / iteration in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &StructureSetHolder> {
        self.entries.iter()
    }

    /// `Structure.biomes()` of a structure referenced by a set of a registry built with [`Self::load`]
    /// (empty for anything else): the callback to hand to
    /// [`ChunkGeneratorStructureState`](super::ChunkGeneratorStructureState)'s constructors.
    pub fn structure_biomes(&self, structure: &Identifier) -> &[BiomeId] {
        self.structure_biomes
            .get(structure)
            .map_or(&[], Vec::as_slice)
    }
}

/// `Structure.biomes()` of a registered structure: its `biomes` field (`Structure.StructureSettings.CODEC`, a
/// `HolderSet<Biome>`). The rest of the structure definition is not decoded.
pub fn structure_biomes(
    registries: &WorldgenRegistries,
    biomes: &BiomeRegistry,
    structure: &Identifier,
) -> LoadResult<Vec<BiomeId>> {
    let json = registries
        .raw(STRUCTURE_REGISTRY, structure)
        .ok_or_else(|| LoadError::new("no such registry entry"))?;
    Fields::new(&json)?.required("biomes", |v| biomes.holder_set_from_json(v))
}
