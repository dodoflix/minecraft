//! `net.minecraft.world.level.block.WeepingVinesPlantBlock` (extends `GrowingPlantBodyBlock`).
use mc_game_core::register_block_class;
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_body_block::{GrowingPlantBody, growing_plant_body_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct WeepingVinesPlantBlock;

impl GrowingPlant for WeepingVinesPlantBlock {
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
impl GrowingPlantBody for WeepingVinesPlantBlock {}

impl BlockBehaviour for WeepingVinesPlantBlock {
    growing_plant_body_behaviour!();
}

register_block_class!("WeepingVinesPlantBlock", |_| Box::new(WeepingVinesPlantBlock));
