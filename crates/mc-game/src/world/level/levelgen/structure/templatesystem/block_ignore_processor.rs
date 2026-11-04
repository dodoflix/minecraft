//! `BlockIgnoreProcessor` (`block_ignore`).

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::*;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use std::sync::Arc;

pub struct BlockIgnoreProcessor {
    to_ignore: Vec<Block>,
}

impl BlockIgnoreProcessor {
    pub fn new(to_ignore: Vec<Block>) -> BlockIgnoreProcessor {
        BlockIgnoreProcessor { to_ignore }
    }
}

impl StructureProcessor for BlockIgnoreProcessor {
    fn process_block(
        &self,
        _level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        if self.to_ignore.contains(&processed_block_info.state.block()) { None } else { Some(processed_block_info) }
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "block_ignore",
    // `WEIRD_BLOCK_STATE_CODEC`: a full `BlockState` codec read down to its block
    decode: |f, _| Ok(Arc::new(BlockIgnoreProcessor::new(f.required("blocks", |v| parse_list(v, |v| parse_block_state(v).map(|s| s.block())))?))),
};
