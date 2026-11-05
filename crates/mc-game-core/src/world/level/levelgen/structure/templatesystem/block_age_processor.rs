//! `BlockAgeProcessor` (`block_age`).

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::codec::*;
use mc_core::direction::Plane;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use std::sync::Arc;

const PROBABILITY_OF_REPLACING_FULL_BLOCK: f32 = 0.5;
const PROBABILITY_OF_REPLACING_STAIRS: f32 = 0.5;
const PROBABILITY_OF_REPLACING_OBSIDIAN: f32 = 0.15;

fn block(name: &str) -> Block {
    Block::by_name(&format!("minecraft:{name}")).unwrap_or_else(|| panic!("unknown block {name}"))
}

fn default_state(name: &str) -> BlockState {
    block(name).default_state()
}

pub struct BlockAgeProcessor {
    mossiness: f32,
}

impl BlockAgeProcessor {
    pub fn new(mossiness: f32) -> BlockAgeProcessor {
        BlockAgeProcessor { mossiness }
    }

    fn maybe_replace_full_stone_block(&self, random: &mut dyn RandomSource) -> Option<BlockState> {
        if random.next_float() >= PROBABILITY_OF_REPLACING_FULL_BLOCK {
            return None;
        }
        // both arrays are built eagerly: their random facings/halves consume the random in this order
        let non_mossy = [default_state("cracked_stone_bricks"), get_random_facing_stairs(random, block("stone_brick_stairs"))];
        let mossy = [default_state("mossy_stone_bricks"), get_random_facing_stairs(random, block("mossy_stone_brick_stairs"))];
        Some(self.get_random_block(random, &non_mossy, &mossy))
    }

    fn maybe_replace_stairs(&self, state: BlockState, random: &mut dyn RandomSource) -> Option<BlockState> {
        if random.next_float() >= PROBABILITY_OF_REPLACING_STAIRS {
            return None;
        }
        let non_mossy = [default_state("stone_slab"), default_state("stone_brick_slab")];
        let mossy = [block("mossy_stone_brick_stairs").default_state().with_properties_of(state), default_state("mossy_stone_brick_slab")];
        Some(self.get_random_block(random, &non_mossy, &mossy))
    }

    fn maybe_replace_slab(&self, state: BlockState, random: &mut dyn RandomSource) -> Option<BlockState> {
        (random.next_float() < self.mossiness).then(|| block("mossy_stone_brick_slab").default_state().with_properties_of(state))
    }

    fn maybe_replace_wall(&self, state: BlockState, random: &mut dyn RandomSource) -> Option<BlockState> {
        (random.next_float() < self.mossiness).then(|| block("mossy_stone_brick_wall").default_state().with_properties_of(state))
    }

    fn maybe_replace_obsidian(&self, random: &mut dyn RandomSource) -> Option<BlockState> {
        (random.next_float() < PROBABILITY_OF_REPLACING_OBSIDIAN).then(|| default_state("crying_obsidian"))
    }

    fn get_random_block(&self, random: &mut dyn RandomSource, non_mossy_blocks: &[BlockState], mossy_blocks: &[BlockState]) -> BlockState {
        if random.next_float() < self.mossiness { get_random_block(random, mossy_blocks) } else { get_random_block(random, non_mossy_blocks) }
    }
}

fn get_random_facing_stairs(random: &mut dyn RandomSource, stair_block: Block) -> BlockState {
    let facing = Plane::Horizontal.get_random_direction(random);
    // `Util.getRandom(Half.values(), random)`: TOP, BOTTOM
    let half = ["top", "bottom"][random.next_int_bounded(2) as usize];
    stair_block.default_state().with_property_str("facing", facing.get_name()).and_then(|s| s.with_property_str("half", half)).expect("stairs have facing and half")
}

fn get_random_block(random: &mut dyn RandomSource, blocks: &[BlockState]) -> BlockState {
    blocks[random.next_int_bounded(blocks.len() as i32) as usize]
}

impl StructureProcessor for BlockAgeProcessor {
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
        let random = &mut *random;
        let state = processed_block_info.state;
        let new_state = if state.is_block(block("stone_bricks")) || state.is_block(block("stone")) || state.is_block(block("chiseled_stone_bricks")) {
            self.maybe_replace_full_stone_block(random)
        } else if state.is_tag("stairs") {
            self.maybe_replace_stairs(state, random)
        } else if state.is_tag("slabs") {
            self.maybe_replace_slab(state, random)
        } else if state.is_tag("walls") {
            self.maybe_replace_wall(state, random)
        } else if state.is_block(block("obsidian")) {
            self.maybe_replace_obsidian(random)
        } else {
            None
        };
        Some(match new_state {
            Some(new_state) => StructureBlockInfo::new(processed_block_info.pos, new_state, processed_block_info.nbt),
            None => processed_block_info,
        })
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "block_age",
    decode: |f, _| Ok(Arc::new(BlockAgeProcessor::new(f.required("mossiness", parse_f32)?))),
};
