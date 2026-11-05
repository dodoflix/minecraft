//! `StructureRegistries`: the decoded `worldgen/structure` registry (+ per-step grouping used by decoration) and the
//! parsed-start cache of a level.

use super::piece::StructurePieceSerializationContext;
use super::pools::structure_template_pool::{StructureTemplatePool, TEMPLATE_POOL_REGISTRY, load_template_pools};
use super::structure::{STRUCTURE_TYPES, Structure, decode_direct};
use super::templatesystem::structure_processor_list::load_processor_lists;
use super::templatesystem::StructureProcessorList;
use super::structure_start::{StartHandle, StructureStart};
use crate::world::level::levelgen::codec::{LoadError, LoadResult};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::generation_step::Decoration;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::{Cells, DecodeCtx, TypedRegistry};
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{WorldgenDataSource, WorldgenRegistries};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const STRUCTURE_REGISTRY: &str = "worldgen/structure";

/// `Registries.STRUCTURE` as typed holders.
pub struct StructureRegistries {
    /// `worldgen/processor_list`.
    pub processor_lists: TypedRegistry<StructureProcessorList>,
    /// `worldgen/template_pool`.
    pub template_pools: TypedRegistry<StructureTemplatePool>,
    pub structures: TypedRegistry<dyn Structure>,
    /// Entries whose `type` has no implementation in `STRUCTURE_TYPES` yet (skipped while the legacy structure types are
    /// ported; empty once every vanilla type is registered, and vanilla would refuse to start with such an entry).
    pub unported: Vec<Identifier>,
    /// `structuresRegistry.stream().collect(groupingBy(step().ordinal()))`: registry order within each step.
    structures_by_step: Vec<Vec<Holder<dyn Structure>>>,
}

impl StructureRegistries {
    /// Decodes `worldgen/structure` from `source`.
    pub fn load(source: Arc<dyn WorldgenDataSource>, worldgen: &WorldgenRegistries, biomes: &BiomeRegistry) -> LoadResult<StructureRegistries> {
        let cells = Cells::new(source);
        let ctx = DecodeCtx { worldgen, biomes, cells: &cells };
        Self::load_with(&ctx)
    }

    /// As [`Self::load`] with a shared decode context (the jigsaw structure resolves pools/processor lists through it).
    pub fn load_with(ctx: &DecodeCtx<'_>) -> LoadResult<StructureRegistries> {
        let processor_lists = load_processor_lists(ctx)?;
        let template_pools = load_template_pools(ctx)?;
        let mut entries = indexmap::IndexMap::new();
        let mut unported = Vec::new();
        for id in ctx.cells.list(STRUCTURE_REGISTRY) {
            let json = ctx.cells.source().get(STRUCTURE_REGISTRY, &id).ok_or_else(|| LoadError::new("no such registry entry").in_entry(STRUCTURE_REGISTRY, &id))?;
            let registered = json.get("type").and_then(|t| t.as_str()).and_then(|t| t.strip_prefix("minecraft:")).is_some_and(|t| STRUCTURE_TYPES.iter().any(|e| e.name == t));
            if !registered {
                unported.push(id);
                continue;
            }
            let value = decode_direct(&json, ctx).map_err(|e| e.in_entry(STRUCTURE_REGISTRY, &id))?;
            let holder = Holder::reference(id.clone(), ctx.cells.cell::<dyn Structure>(STRUCTURE_REGISTRY, &id));
            holder.bind(value);
            entries.insert(id, holder);
        }
        let structures = TypedRegistry::new(STRUCTURE_REGISTRY, entries);
        ctx.cells.check_all_bound::<dyn Structure>(STRUCTURE_REGISTRY)?;
        let mut structures_by_step = vec![Vec::new(); Decoration::ALL.len()];
        for (_, holder) in structures.iter() {
            structures_by_step[holder.value().step().ordinal()].push(holder.clone());
        }
        Ok(StructureRegistries { processor_lists, template_pools, structures, unported, structures_by_step })
    }

    /// A registry without structures.
    pub fn empty() -> StructureRegistries {
        StructureRegistries {
            processor_lists: TypedRegistry::new("worldgen/processor_list", indexmap::IndexMap::new()),
            template_pools: TypedRegistry::new(TEMPLATE_POOL_REGISTRY, indexmap::IndexMap::new()),
            structures: TypedRegistry::new(STRUCTURE_REGISTRY, indexmap::IndexMap::new()),
            unported: Vec::new(),
            structures_by_step: vec![Vec::new(); Decoration::ALL.len()] }
    }

    pub fn get(&self, id: &Identifier) -> Option<&Holder<dyn Structure>> {
        self.structures.get(id)
    }

    /// The structures of one generation step in registry order.
    pub fn structures_for_step(&self, step: usize) -> &[Holder<dyn Structure>] {
        self.structures_by_step.get(step).map_or(&[], Vec::as_slice)
    }
}

const START_CACHE_CAPACITY: usize = 8192;

struct CacheEntry {
    /// The NBT the start was parsed from: the entry is valid while the chunk still holds exactly this tag.
    source: CompoundTag,
    start: Option<StartHandle>,
}

/// Parsed `StructureStart`s keyed by (origin chunk, structure id). Chunks store raw NBT; features ask for the starts of a
/// chunk once per structure and step, so the parse is shared. A lookup re-validates against the chunk's current tag, so
/// regenerated/replaced chunks never serve stale starts.
#[derive(Default)]
pub struct StartCache {
    entries: Mutex<HashMap<(i64, String), CacheEntry>>,
}

impl StartCache {
    /// `chunk.getStartForStructure(structure)`: `None` if the chunk has no start for it (or it fails to load); INVALID
    /// starts are returned as invalid handles.
    pub fn get_or_parse(&self, env: &GenEnv, chunk_pos: ChunkPos, structure_id: &str, raw: &CompoundTag) -> Option<StartHandle> {
        let key = (chunk_pos.pack(), structure_id.to_owned());
        {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = entries.get(&key)
                && entry.source == *raw
            {
                return entry.start.clone();
            }
        }
        let context = StructurePieceSerializationContext { env };
        let start = match StructureStart::load_static_start(&context, raw, env.seed) {
            Ok(start) => Some(StartHandle::new(start)),
            Err(message) => {
                log::error!("{message}");
                None
            }
        };
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= START_CACHE_CAPACITY {
            entries.clear();
        }
        entries.insert(key, CacheEntry { source: raw.clone(), start: start.clone() });
        start
    }

    /// Records that the chunk's tag for `structure_id` is now `raw` and the live start is `start` (after the start
    /// was created, or its state was written back).
    pub fn put(&self, chunk_pos: ChunkPos, structure_id: &str, raw: CompoundTag, start: StartHandle) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= START_CACHE_CAPACITY {
            entries.clear();
        }
        entries.insert((chunk_pos.pack(), structure_id.to_owned()), CacheEntry { source: raw, start: Some(start) });
    }
}
