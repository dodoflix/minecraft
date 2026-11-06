//! `BlockRotProcessor` (`block_rot`).

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::*;
use mc_core::pos::BlockPos;
use std::sync::Arc;

pub struct BlockRotProcessor {
    rottable_blocks: Option<BlockSet>,
    integrity: f32,
}

impl BlockRotProcessor {
    pub fn new(rottable_blocks: Option<BlockSet>, integrity: f32) -> BlockRotProcessor {
        BlockRotProcessor {
            rottable_blocks,
            integrity,
        }
    }
}

impl StructureProcessor for BlockRotProcessor {
    fn process_block(
        &self,
        _level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        let mut random = settings.get_random(Some(processed_block_info.pos));
        let rots = self
            .rottable_blocks
            .as_ref()
            .is_none_or(|set| set.contains(processed_block_info.state.block()));
        if rots && !(random.next_float() <= self.integrity) {
            None
        } else {
            Some(processed_block_info)
        }
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "block_rot",
    decode: |f, _| {
        let rottable_blocks = f.optional("rottable_blocks", BlockSet::from_json)?;
        let integrity = f.required("integrity", |v| parse_float_range(v, 0.0, 1.0))?;
        Ok(Arc::new(BlockRotProcessor::new(rottable_blocks, integrity)))
    },
};
