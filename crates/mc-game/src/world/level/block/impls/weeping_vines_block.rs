//! `net.minecraft.world.level.block.WeepingVinesBlock` (extends `GrowingPlantHeadBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_head_block::{self, GrowingPlantHead};
use super::prelude::*;

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
}

impl BlockBehaviour for WeepingVinesBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_head_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("WeepingVinesBlock", |_| Box::new(WeepingVinesBlock));
