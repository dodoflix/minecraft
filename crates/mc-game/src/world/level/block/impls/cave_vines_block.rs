//! `net.minecraft.world.level.block.CaveVinesBlock` (extends `GrowingPlantHeadBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_head_block::{self, GrowingPlantHead};
use super::prelude::*;

pub struct CaveVinesBlock;

impl GrowingPlant for CaveVinesBlock {
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
impl GrowingPlantHead for CaveVinesBlock {
    fn update_body_after_converted_from_head(&self, head_state: BlockState, body_state: BlockState) -> BlockState {
        body_state.set(props::BERRIES, head_state.get(props::BERRIES))
    }
}

impl BlockBehaviour for CaveVinesBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_head_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("CaveVinesBlock", |_| Box::new(CaveVinesBlock));
