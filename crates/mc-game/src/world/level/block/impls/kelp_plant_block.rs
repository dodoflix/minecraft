//! `net.minecraft.world.level.block.KelpPlantBlock` (extends `GrowingPlantBodyBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_body_block::{self, GrowingPlantBody};
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
impl GrowingPlantBody for KelpPlantBlock {
}

impl BlockBehaviour for KelpPlantBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_body_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("KelpPlantBlock", |_| Box::new(KelpPlantBlock));
