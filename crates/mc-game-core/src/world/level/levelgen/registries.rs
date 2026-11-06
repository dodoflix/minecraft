//! The data-driven worldgen registries of the feature layer (`worldgen/feature`, `worldgen/placed_feature`,
//! `worldgen/block_state_provider`) decoded into typed values, and the [`DecodeCtx`] every codec of this module tree
//! decodes with (holder resolution + the shared `mc_worldgen` registries).
//!
//! Entries reference each other through [`Holder`]s whose cells are bound once the registry is fully decoded, so decode
//! order does not matter and cycles are allowed (vanilla's unbound `Holder.Reference`s). The structure layer
//! (`structure::StructureRegistries`) is built the same way from the same [`Cells`].

use super::feature::Feature;
use super::holder::Holder;
use super::placement::PlacedFeature;
use super::stateproviders::BlockStateProvider;
use super::structure::templatesystem::StructureProcessorList;
use super::structure::templatesystem::structure_processor_list::{
    PROCESSOR_LIST_REGISTRY, load_processor_lists,
};
use crate::world::item::block_transformer::{BLOCK_TRANSFORMER_REGISTRY, BlockTransformer};
use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{WorldgenDataSource, WorldgenRegistries};
use mc_worldgen::json::{LoadError, LoadResult};
use serde_json::Value;
use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

pub const FEATURE_REGISTRY: &str = "worldgen/feature";
pub const PLACED_FEATURE_REGISTRY: &str = "worldgen/placed_feature";
pub const BLOCK_STATE_PROVIDER_REGISTRY: &str = "worldgen/block_state_provider";

/// A decoded registry: entry id (sorted by `namespace:path`) -> bound holder.
pub struct TypedRegistry<T: ?Sized> {
    pub key: &'static str,
    entries: IndexMap<Identifier, Holder<T>>,
}

impl<T: ?Sized> TypedRegistry<T> {
    pub fn new(key: &'static str, entries: IndexMap<Identifier, Holder<T>>) -> Self {
        Self { key, entries }
    }

    pub fn get(&self, id: &Identifier) -> Option<&Holder<T>> {
        self.entries.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Identifier, &Holder<T>)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Holder cells shared by every registry that is decoded against the same data source.
pub struct Cells {
    source: Arc<dyn WorldgenDataSource>,
    known: RefCell<HashMap<&'static str, HashSet<Identifier>>>,
    cells: RefCell<HashMap<(&'static str, Identifier), Arc<dyn Any + Send + Sync>>>,
}

impl Cells {
    pub fn new(source: Arc<dyn WorldgenDataSource>) -> Cells {
        Cells {
            source,
            known: RefCell::new(HashMap::new()),
            cells: RefCell::new(HashMap::new()),
        }
    }

    pub fn source(&self) -> &Arc<dyn WorldgenDataSource> {
        &self.source
    }

    /// The ids of `registry` in the data source, sorted by `namespace:path` (the registry's entry order).
    pub fn list(&self, registry: &'static str) -> Vec<Identifier> {
        let mut ids = self.source.list(registry);
        ids.sort_by_key(|id| id.to_string());
        self.known
            .borrow_mut()
            .entry(registry)
            .or_insert_with(|| ids.iter().cloned().collect());
        ids
    }

    fn exists(&self, registry: &'static str, id: &Identifier) -> bool {
        if !self.known.borrow().contains_key(registry) {
            self.list(registry);
        }
        self.known
            .borrow()
            .get(registry)
            .is_some_and(|set| set.contains(id))
    }

    /// The shared cell of entry `id` (created on first use).
    pub fn cell<T: ?Sized + Send + Sync + 'static>(
        &self,
        registry: &'static str,
        id: &Identifier,
    ) -> Arc<OnceLock<Arc<T>>> {
        let mut cells = self.cells.borrow_mut();
        let any = cells
            .entry((registry, id.clone()))
            .or_insert_with(|| Arc::new(OnceLock::<Arc<T>>::new()) as Arc<dyn Any + Send + Sync>);
        any.clone()
            .downcast::<OnceLock<Arc<T>>>()
            .unwrap_or_else(|_| panic!("registry {registry} entry {id} holds a different type"))
    }

    /// Fails when a referenced entry was never bound (it does not exist in the data source).
    pub fn check_all_bound<T: ?Sized + Send + Sync + 'static>(
        &self,
        registry: &'static str,
    ) -> LoadResult<()> {
        for ((reg, id), any) in self.cells.borrow().iter() {
            if *reg == registry
                && let Ok(cell) = any.clone().downcast::<OnceLock<Arc<T>>>()
                && cell.get().is_none()
            {
                return Err(LoadError::new(format!(
                    "Unbound values in registry ResourceKey[minecraft:root / minecraft:{registry}]: [{id}]"
                )));
            }
        }
        Ok(())
    }
}

/// What the codecs of features, placement modifiers, predicates, providers (and structures) decode with.
pub struct DecodeCtx<'a> {
    pub worldgen: &'a WorldgenRegistries,
    pub biomes: &'a BiomeRegistry,
    pub cells: &'a Cells,
}

impl<'a> DecodeCtx<'a> {
    /// `RegistryFileCodec` reference: the entry `id` of `registry` must exist.
    pub fn holder<T: ?Sized + Send + Sync + 'static>(
        &self,
        registry: &'static str,
        id: &Identifier,
    ) -> LoadResult<Holder<T>> {
        if !self.cells.exists(registry, id) {
            return Err(LoadError::new(format!(
                "Failed to get element ResourceKey[minecraft:{registry} / {id}]"
            )));
        }
        Ok(Holder::reference(
            id.clone(),
            self.cells.cell::<T>(registry, id),
        ))
    }

    /// `RegistryCodecs.holder(registry, direct)`: a string is a reference, anything else a direct (inline) definition.
    pub fn holder_or_direct<T: ?Sized + Send + Sync + 'static>(
        &self,
        registry: &'static str,
        value: &Value,
        direct: impl FnOnce(&Value) -> LoadResult<Arc<T>>,
    ) -> LoadResult<Holder<T>> {
        match value {
            Value::String(s) => {
                self.holder(registry, &Identifier::read(s).map_err(LoadError::new)?)
            }
            other => Ok(Holder::direct(direct(other)?)),
        }
    }

    /// `Feature.CODEC`.
    pub fn feature(&self, value: &Value) -> LoadResult<Holder<dyn Feature>> {
        self.holder_or_direct(FEATURE_REGISTRY, value, |v| {
            super::feature::decode_direct(v, self)
        })
    }

    /// `PlacedFeature.CODEC`.
    pub fn placed_feature(&self, value: &Value) -> LoadResult<Holder<PlacedFeature>> {
        self.holder_or_direct(PLACED_FEATURE_REGISTRY, value, |v| {
            PlacedFeature::decode_direct(v, self).map(Arc::new)
        })
    }

    /// `BlockStateProvider.CODEC`.
    pub fn block_state_provider(
        &self,
        value: &Value,
    ) -> LoadResult<Holder<dyn BlockStateProvider>> {
        self.holder_or_direct(BLOCK_STATE_PROVIDER_REGISTRY, value, |v| {
            super::stateproviders::decode_direct(v, self)
        })
    }
}

/// Decodes every entry of `registry` with `decode` and binds the holder cells.
pub fn load_registry<T: ?Sized + Send + Sync + 'static>(
    ctx: &DecodeCtx,
    registry: &'static str,
    decode: impl Fn(&Value, &DecodeCtx) -> LoadResult<Arc<T>>,
) -> LoadResult<TypedRegistry<T>> {
    let mut entries = IndexMap::new();
    for id in ctx.cells.list(registry) {
        let json = ctx
            .cells
            .source()
            .get(registry, &id)
            .ok_or_else(|| LoadError::new("no such registry entry").in_entry(registry, &id))?;
        let value = decode(&json, ctx).map_err(|e| e.in_entry(registry, &id))?;
        let holder = Holder::reference(id.clone(), ctx.cells.cell::<T>(registry, &id));
        holder.bind(value);
        entries.insert(id, holder);
    }
    Ok(TypedRegistry::new(registry, entries))
}

/// The feature-layer registries of one data pack stack.
pub struct GenRegistries {
    pub features: TypedRegistry<dyn Feature>,
    pub placed_features: TypedRegistry<PlacedFeature>,
    pub block_state_providers: TypedRegistry<dyn BlockStateProvider>,
    /// `block_transformer` (axes/hoes/shovels): its rules are block predicates + block state providers.
    pub block_transformers: TypedRegistry<BlockTransformer>,
}

impl GenRegistries {
    /// Decodes the three registries from `source`. Entry errors are `<registry>/<id>: <field path>: <message>`.
    pub fn load(
        source: Arc<dyn WorldgenDataSource>,
        worldgen: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<GenRegistries> {
        let cells = Cells::new(source);
        let ctx = DecodeCtx {
            worldgen,
            biomes,
            cells: &cells,
        };
        // `fossil`/`template` features reference `worldgen/processor_list` entries: their holders bind through these cells
        // (the structure layer decodes its own copy for its templates/pools).
        load_processor_lists(&ctx)?;
        let block_state_providers = load_registry(&ctx, BLOCK_STATE_PROVIDER_REGISTRY, |v, c| {
            super::stateproviders::decode_direct(v, c)
        })?;
        let features = load_registry(&ctx, FEATURE_REGISTRY, |v, c| {
            super::feature::decode_direct(v, c)
        })?;
        let placed_features = load_registry(&ctx, PLACED_FEATURE_REGISTRY, |v, c| {
            PlacedFeature::decode_direct(v, c).map(Arc::new)
        })?;
        let block_transformers = load_registry(&ctx, BLOCK_TRANSFORMER_REGISTRY, |v, c| {
            BlockTransformer::decode(v, c)
        })?;
        cells.check_all_bound::<dyn Feature>(FEATURE_REGISTRY)?;
        cells.check_all_bound::<PlacedFeature>(PLACED_FEATURE_REGISTRY)?;
        cells.check_all_bound::<dyn BlockStateProvider>(BLOCK_STATE_PROVIDER_REGISTRY)?;
        cells.check_all_bound::<StructureProcessorList>(PROCESSOR_LIST_REGISTRY)?;
        Ok(GenRegistries {
            features,
            placed_features,
            block_state_providers,
            block_transformers,
        })
    }

    /// `Feature.DIRECT_CODEC` of an inline definition (`/place feature {...}`): decoded with holders into the registries
    /// decoded from `worldgen`'s source.
    /// shortcut: re-decodes all three registries per call (holder cells are per decode), fine for a command; cache the
    /// cells in `GenRegistries` if an inline definition is ever needed on a hot path.
    pub fn decode_inline_feature(
        worldgen: &WorldgenRegistries,
        biomes: &BiomeRegistry,
        value: &Value,
    ) -> LoadResult<Holder<dyn Feature>> {
        let cells = Cells::new(worldgen.source().clone());
        let ctx = DecodeCtx {
            worldgen,
            biomes,
            cells: &cells,
        };
        load_processor_lists(&ctx)?;
        load_registry(&ctx, BLOCK_STATE_PROVIDER_REGISTRY, |v, c| {
            super::stateproviders::decode_direct(v, c)
        })?;
        load_registry(&ctx, FEATURE_REGISTRY, |v, c| {
            super::feature::decode_direct(v, c)
        })?;
        load_registry(&ctx, PLACED_FEATURE_REGISTRY, |v, c| {
            PlacedFeature::decode_direct(v, c).map(Arc::new)
        })?;
        let feature = Holder::direct(super::feature::decode_direct(value, &ctx)?);
        cells.check_all_bound::<dyn Feature>(FEATURE_REGISTRY)?;
        cells.check_all_bound::<PlacedFeature>(PLACED_FEATURE_REGISTRY)?;
        cells.check_all_bound::<dyn BlockStateProvider>(BLOCK_STATE_PROVIDER_REGISTRY)?;
        cells.check_all_bound::<StructureProcessorList>(PROCESSOR_LIST_REGISTRY)?;
        Ok(feature)
    }

    pub fn feature(&self, id: &Identifier) -> Option<&Holder<dyn Feature>> {
        self.features.get(id)
    }

    pub fn placed_feature(&self, id: &Identifier) -> Option<&Holder<PlacedFeature>> {
        self.placed_features.get(id)
    }
}

/// Loads the registries of the vanilla data pack under `ref/src/data` (golden tests / tools).
pub fn load_vanilla_data() -> (
    Arc<WorldgenRegistries>,
    Arc<BiomeRegistry>,
    LoadResult<GenRegistries>,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let source: Arc<dyn WorldgenDataSource> = Arc::new(mc_worldgen::data::FsDataSource::new(root));
    let worldgen = Arc::new(WorldgenRegistries::new(source.clone()));
    let biomes = Arc::new(BiomeRegistry::load(&worldgen).expect("vanilla biomes load"));
    let registries = GenRegistries::load(source, &worldgen, &biomes);
    (worldgen, biomes, registries)
}
