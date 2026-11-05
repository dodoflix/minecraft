//! `net.minecraft.world.level.block.TallGrassBlock` (extends `VegetationBlock`; `short_grass`, `fern`): bonemeal grows the
//! double plant.
use super::double_plant_block;
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour_methods};
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct TallGrassBlock;
impl MayPlaceOn for TallGrassBlock {}

/// `getGrownBlock(state)`.
fn get_grown_block(state: BlockState) -> Block {
    if state.is_block(Block::FERN) { Block::LARGE_FERN } else { Block::TALL_GRASS }
}

impl BlockBehaviour for TallGrassBlock {
    vegetation_behaviour_methods!();
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        get_grown_block(state).default_state().can_survive(level, pos) && level.is_empty_block(pos.above()) && !level.is_outside_build_height(pos.above().y())
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        double_plant_block::place_at(level, get_grown_block(state).default_state(), pos, 2);
    }
}

register_block_class!("TallGrassBlock", |_| Box::new(TallGrassBlock));
