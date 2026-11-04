//! `net.minecraft.world.level.block.CaveVinesPlantBlock` (extends `GrowingPlantBodyBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_body_block::{self, GrowingPlantBody};
use super::prelude::*;

pub struct CaveVinesPlantBlock;

impl GrowingPlant for CaveVinesPlantBlock {
    fn growth_direction(&self) -> Direction {
        Direction::Down
    }
    fn schedule_fluid_ticks(&self) -> bool {
        false
    }
    fn head_block(&self) -> Block {
        Block::CAVE_VINES
    }
    fn body_block(&self) -> Block {
        Block::CAVE_VINES_PLANT
    }
}
impl GrowingPlantBody for CaveVinesPlantBlock {
    fn update_head_after_converted_from_body(&self, body_state: BlockState, head_state: BlockState) -> BlockState {
        head_state.set(props::BERRIES, body_state.get(props::BERRIES))
    }
}

impl BlockBehaviour for CaveVinesPlantBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_body_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("CaveVinesPlantBlock", |_| Box::new(CaveVinesPlantBlock));
