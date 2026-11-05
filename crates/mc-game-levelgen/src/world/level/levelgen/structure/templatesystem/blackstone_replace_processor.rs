//! `BlackstoneReplaceProcessor` (`blackstone_replace`).

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

const REPLACEMENTS: [(&str, &str); 23] = [
    ("cobblestone", "blackstone"),
    ("mossy_cobblestone", "blackstone"),
    ("stone", "polished_blackstone"),
    ("stone_bricks", "polished_blackstone_bricks"),
    ("mossy_stone_bricks", "polished_blackstone_bricks"),
    ("cobblestone_stairs", "blackstone_stairs"),
    ("mossy_cobblestone_stairs", "blackstone_stairs"),
    ("stone_stairs", "polished_blackstone_stairs"),
    ("stone_brick_stairs", "polished_blackstone_brick_stairs"),
    ("mossy_stone_brick_stairs", "polished_blackstone_brick_stairs"),
    ("cobblestone_slab", "blackstone_slab"),
    ("mossy_cobblestone_slab", "blackstone_slab"),
    ("smooth_stone_slab", "polished_blackstone_slab"),
    ("stone_slab", "polished_blackstone_slab"),
    ("stone_brick_slab", "polished_blackstone_brick_slab"),
    ("mossy_stone_brick_slab", "polished_blackstone_brick_slab"),
    ("stone_brick_wall", "polished_blackstone_brick_wall"),
    ("mossy_stone_brick_wall", "polished_blackstone_brick_wall"),
    ("cobblestone_wall", "blackstone_wall"),
    ("mossy_cobblestone_wall", "blackstone_wall"),
    ("chiseled_stone_bricks", "chiseled_polished_blackstone"),
    ("cracked_stone_bricks", "cracked_polished_blackstone_bricks"),
    ("iron_bars", "iron_chain"),
];

static MAP: LazyLock<HashMap<Block, Block>> = LazyLock::new(|| {
    let block = |name: &str| Block::by_name(&format!("minecraft:{name}")).unwrap_or_else(|| panic!("unknown block {name}"));
    REPLACEMENTS.iter().map(|(from, to)| (block(from), block(to))).collect()
});

pub struct BlackstoneReplaceProcessor;

impl StructureProcessor for BlackstoneReplaceProcessor {
    fn process_block(
        &self,
        _level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        let Some(new_block) = MAP.get(&processed_block_info.state.block()).copied() else { return Some(processed_block_info) };
        let old_state = processed_block_info.state;
        let mut new_state = new_block.default_state();
        // `StairBlock.FACING` (horizontal), `StairBlock.HALF`, `SlabBlock.TYPE`: the replaced blocks only have these
        // properties under those names
        for property in ["facing", "half", "type"] {
            if let Some(value) = old_state.get_property_str(property)
                && let Some(updated) = new_state.with_property_str(property, value)
            {
                new_state = updated;
            }
        }
        Some(StructureBlockInfo::new(processed_block_info.pos, new_state, processed_block_info.nbt))
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry { name: "blackstone_replace", decode: |_, _| Ok(Arc::new(BlackstoneReplaceProcessor)) };
