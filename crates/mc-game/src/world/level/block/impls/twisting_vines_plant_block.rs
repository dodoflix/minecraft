//! `net.minecraft.world.level.block.TwistingVinesPlantBlock` (extends `GrowingPlantBodyBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_body_block::{self, GrowingPlantBody};
use super::prelude::*;

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
impl GrowingPlantBody for TwistingVinesPlantBlock {
}

impl BlockBehaviour for TwistingVinesPlantBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_body_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("TwistingVinesPlantBlock", |_| Box::new(TwistingVinesPlantBlock));
