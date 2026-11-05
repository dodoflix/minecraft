//! `net.minecraft.world.level.block.KelpPlantBlock` (extends `GrowingPlantBodyBlock`).
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_body_block::{GrowingPlantBody, growing_plant_body_behaviour};
use super::prelude::*;

pub struct KelpPlantBlock;

impl GrowingPlant for KelpPlantBlock {
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
impl GrowingPlantBody for KelpPlantBlock {}

impl BlockBehaviour for KelpPlantBlock {
    growing_plant_body_behaviour!();
}

register_block_class!("KelpPlantBlock", |_| Box::new(KelpPlantBlock));
