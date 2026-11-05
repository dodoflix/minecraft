//! `net.minecraft.world.level.block.KelpPlantBlock` (extends `GrowingPlantBodyBlock`).
use mc_game_core::register_block_class;
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_body_block::{GrowingPlantBody, growing_plant_body_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;

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
    // LiquidBlockContainer: canPlaceLiquid/placeLiquid are `false` (trait defaults).
    fn is_liquid_block_container(&self) -> bool {
        true
    }
}

register_block_class!("KelpPlantBlock", |_| Box::new(KelpPlantBlock));
