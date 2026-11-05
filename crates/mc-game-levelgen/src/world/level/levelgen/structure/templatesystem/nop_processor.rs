//! `NopProcessor` (`nop`).

use mc_game_core::world::level::levelgen::structure::templatesystem::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use std::sync::Arc;

pub struct NopProcessor;

impl StructureProcessor for NopProcessor {}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry { name: "nop", decode: |_, _| Ok(Arc::new(NopProcessor)) };
