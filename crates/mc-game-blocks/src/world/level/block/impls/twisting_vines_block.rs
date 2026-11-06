//! `net.minecraft.world.level.block.TwistingVinesBlock` (extends `GrowingPlantHeadBlock`).
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_head_block::{GrowingPlantHead, growing_plant_head_behaviour};
use super::nether_vines;
use mc_core::random::RandomSource;
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct TwistingVinesBlock;

impl GrowingPlant for TwistingVinesBlock {
    fn growth_direction(&self) -> Direction {
        Direction::Up
    }
    fn schedule_fluid_ticks(&self) -> bool {
        false
    }
    fn head_block(&self) -> Block {
        Block::TWISTING_VINES
    }
    fn body_block(&self) -> Block {
        Block::TWISTING_VINES_PLANT
    }
}
impl GrowingPlantHead for TwistingVinesBlock {
    fn grow_per_tick_probability(&self) -> f64 {
        nether_vines::GROW_PER_TICK_PROBABILITY
    }
    fn blocks_to_grow_when_bonemealed(&self, random: &mut dyn RandomSource) -> i32 {
        nether_vines::get_blocks_to_grow_when_bonemealed(random)
    }
    fn can_grow_into(&self, state: BlockState) -> bool {
        nether_vines::is_valid_growth_state(state)
    }
}

impl BlockBehaviour for TwistingVinesBlock {
    growing_plant_head_behaviour!();
}

register_block_class!("TwistingVinesBlock", |_| Box::new(TwistingVinesBlock));
