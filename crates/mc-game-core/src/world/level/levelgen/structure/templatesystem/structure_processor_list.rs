//! `StructureProcessorList` and the `worldgen/processor_list` registry.

use super::structure_processor::{StructureProcessor, decode_direct_list};
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::{DecodeCtx, TypedRegistry, load_registry};
use mc_worldgen::json::LoadResult;
use serde_json::Value;
use std::sync::Arc;

pub const PROCESSOR_LIST_REGISTRY: &str = "worldgen/processor_list";

pub struct StructureProcessorList {
    list: Vec<Arc<dyn StructureProcessor>>,
}

impl StructureProcessorList {
    pub fn new(list: Vec<Arc<dyn StructureProcessor>>) -> StructureProcessorList {
        StructureProcessorList { list }
    }

    /// `list()`.
    pub fn list(&self) -> &[Arc<dyn StructureProcessor>] {
        &self.list
    }
}

impl DecodeCtx<'_> {
    /// `StructureProcessorType.LIST_CODEC` (`RegistryCodecs.holder(PROCESSOR_LIST, DIRECT_CODEC)`): a string is a
    /// reference, anything else an inline list.
    pub fn processor_list(&self, value: &Value) -> LoadResult<Holder<StructureProcessorList>> {
        self.holder_or_direct(PROCESSOR_LIST_REGISTRY, value, |v| decode_direct_list(v, self).map(Arc::new))
    }
}

/// Decodes every `worldgen/processor_list` entry of the data source behind `ctx` and binds the holders.
pub fn load_processor_lists(ctx: &DecodeCtx<'_>) -> LoadResult<TypedRegistry<StructureProcessorList>> {
    let registry = load_registry(ctx, PROCESSOR_LIST_REGISTRY, |v, c| decode_direct_list(v, c).map(Arc::new))?;
    ctx.cells.check_all_bound::<StructureProcessorList>(PROCESSOR_LIST_REGISTRY)?;
    Ok(registry)
}
