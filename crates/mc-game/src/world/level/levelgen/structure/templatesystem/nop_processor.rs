//! `NopProcessor` (`nop`).

use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use crate::world::level::levelgen::codec::TypeEntry;
use std::sync::Arc;

pub struct NopProcessor;

impl StructureProcessor for NopProcessor {}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry { name: "nop", decode: |_, _| Ok(Arc::new(NopProcessor)) };
