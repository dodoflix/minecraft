//! `BlockIgnoreProcessor` (`block_ignore`).

use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_place_settings::StructurePlaceSettings;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_processor::{
    STRUCTURE_PROCESSOR_TYPES, StructureProcessor,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::StructureBlockInfo;
use std::sync::{Arc, LazyLock};

pub struct BlockIgnoreProcessor {
    to_ignore: Vec<Block>,
}

impl BlockIgnoreProcessor {
    pub fn new(to_ignore: Vec<Block>) -> BlockIgnoreProcessor {
        BlockIgnoreProcessor { to_ignore }
    }
}

fn shared(blocks: &[&str]) -> Arc<dyn StructureProcessor> {
    Arc::new(BlockIgnoreProcessor::new(
        blocks
            .iter()
            .map(|b| Block::by_name(b).expect("vanilla block"))
            .collect(),
    ))
}

/// `BlockIgnoreProcessor.STRUCTURE_BLOCK` (one shared instance, so `popProcessor` can find it by identity).
pub static STRUCTURE_BLOCK: LazyLock<Arc<dyn StructureProcessor>> =
    LazyLock::new(|| shared(&["minecraft:structure_block"]));
/// `BlockIgnoreProcessor.AIR`.
pub static AIR: LazyLock<Arc<dyn StructureProcessor>> =
    LazyLock::new(|| shared(&["minecraft:air"]));
/// `BlockIgnoreProcessor.STRUCTURE_AND_AIR`.
pub static STRUCTURE_AND_AIR: LazyLock<Arc<dyn StructureProcessor>> =
    LazyLock::new(|| shared(&["minecraft:air", "minecraft:structure_block"]));

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
        if self.to_ignore.contains(&processed_block_info.state.block()) {
            None
        } else {
            Some(processed_block_info)
        }
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "block_ignore",
    // `WEIRD_BLOCK_STATE_CODEC`: a full `BlockState` codec read down to its block
    decode: |f, _| {
        Ok(Arc::new(BlockIgnoreProcessor::new(
            f.required("blocks", |v| {
                parse_list(v, |v| parse_block_state(v).map(|s| s.block()))
            })?,
        )))
    },
};
