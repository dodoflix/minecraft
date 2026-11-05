//! `ProtectedBlockProcessor` (`protected_blocks`).

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::*;
use mc_core::pos::BlockPos;
use std::sync::Arc;

pub struct ProtectedBlockProcessor {
    pub cannot_replace: BlockSet,
}

impl StructureProcessor for ProtectedBlockProcessor {
    fn process_block(
        &self,
        level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        if self.cannot_replace.contains(level.get_block_state(processed_block_info.pos).block()) { None } else { Some(processed_block_info) }
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "protected_blocks",
    decode: |f, _| Ok(Arc::new(ProtectedBlockProcessor { cannot_replace: f.required("value", BlockSet::from_json)? })),
};
