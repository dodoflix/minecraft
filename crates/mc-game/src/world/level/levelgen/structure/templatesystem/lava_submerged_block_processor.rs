//! `LavaSubmergedBlockProcessor` (`lava_submerged_block`).

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers::is_shape_full;
use crate::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use std::sync::Arc;

pub struct LavaSubmergedBlockProcessor;

impl StructureProcessor for LavaSubmergedBlockProcessor {
    fn process_block(
        &self,
        level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        let pos = processed_block_info.pos;
        let lava = Block::by_name("minecraft:lava").expect("lava block");
        let was_lava_before = level.get_block_state(pos).block() == lava;
        if was_lava_before && !is_shape_full(&processed_block_info.state.get_shape(level, pos)) {
            Some(StructureBlockInfo::new(pos, lava.default_state(), processed_block_info.nbt))
        } else {
            Some(processed_block_info)
        }
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry { name: "lava_submerged_block", decode: |_, _| Ok(Arc::new(LavaSubmergedBlockProcessor)) };
