//! `net.minecraft.world.level.block.KelpBlock` (extends `GrowingPlantHeadBlock`).
use mc_game_core::register_block_class;
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_head_block::{GrowingPlantHead, growing_plant_head_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_core::random::RandomSource;

pub struct KelpBlock;

impl GrowingPlant for KelpBlock {
    fn growth_direction(&self) -> Direction {
        Direction::Up
    }
    fn schedule_fluid_ticks(&self) -> bool {
        true
    }
    fn head_block(&self) -> Block {
        Block::KELP
    }
    fn body_block(&self) -> Block {
        Block::KELP_PLANT
    }
    fn can_attach_to(&self, state: BlockState) -> bool {
        !state.is_tag("cannot_support_kelp")
    }
}
impl GrowingPlantHead for KelpBlock {
    fn grow_per_tick_probability(&self) -> f64 {
        0.14
    }
    fn blocks_to_grow_when_bonemealed(&self, _random: &mut dyn RandomSource) -> i32 {
        1
    }
    fn can_grow_into(&self, state: BlockState) -> bool {
        state.is_block(Block::WATER)
    }
    fn can_be_placed_in(&self, fluid_state: mc_data::fluid::FluidState) -> bool {
        fluid_state.is_tag("water") && fluid_state.is_full()
    }
}

impl BlockBehaviour for KelpBlock {
    growing_plant_head_behaviour!();
    // LiquidBlockContainer: canPlaceLiquid/placeLiquid are `false` (trait defaults).
    fn is_liquid_block_container(&self) -> bool {
        true
    }
}

register_block_class!("KelpBlock", |_| Box::new(KelpBlock));
