//! `net.minecraft.world.level.block.WeepingVinesBlock` (extends `GrowingPlantHeadBlock`).
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_head_block::{GrowingPlantHead, growing_plant_head_behaviour};
use super::nether_vines;
use mc_core::random::RandomSource;
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct WeepingVinesBlock;

impl GrowingPlant for WeepingVinesBlock {
    fn growth_direction(&self) -> Direction {
        Direction::Down
    }
    fn schedule_fluid_ticks(&self) -> bool {
        false
    }
    fn head_block(&self) -> Block {
        Block::WEEPING_VINES
    }
    fn body_block(&self) -> Block {
        Block::WEEPING_VINES_PLANT
    }
}
impl GrowingPlantHead for WeepingVinesBlock {
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

impl BlockBehaviour for WeepingVinesBlock {
    growing_plant_head_behaviour!();
}

register_block_class!("WeepingVinesBlock", |_| Box::new(WeepingVinesBlock));
