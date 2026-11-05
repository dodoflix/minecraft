//! `net.minecraft.world.level.block.TwistingVinesPlantBlock` (extends `GrowingPlantBodyBlock`).
use mc_game_core::register_block_class;
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_body_block::{GrowingPlantBody, growing_plant_body_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct TwistingVinesPlantBlock;

impl GrowingPlant for TwistingVinesPlantBlock {
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
impl GrowingPlantBody for TwistingVinesPlantBlock {}

impl BlockBehaviour for TwistingVinesPlantBlock {
    growing_plant_body_behaviour!();
}

register_block_class!("TwistingVinesPlantBlock", |_| Box::new(TwistingVinesPlantBlock));
