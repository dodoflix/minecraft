//! `net.minecraft.world.level.block.WeepingVinesPlantBlock` (extends `GrowingPlantBodyBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_body_block::{self, GrowingPlantBody};
use super::prelude::*;

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
impl GrowingPlantBody for WeepingVinesPlantBlock {
}

impl BlockBehaviour for WeepingVinesPlantBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_body_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("WeepingVinesPlantBlock", |_| Box::new(WeepingVinesPlantBlock));
