//! `net.minecraft.world.level.block.TwistingVinesBlock` (extends `GrowingPlantHeadBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_head_block::{self, GrowingPlantHead};
use super::prelude::*;

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
}

impl BlockBehaviour for TwistingVinesBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_head_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("TwistingVinesBlock", |_| Box::new(TwistingVinesBlock));
