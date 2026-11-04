//! `net.minecraft.world.level.block.KelpBlock` (extends `GrowingPlantHeadBlock`).
use super::growing_plant_block::{self, GrowingPlant};
use super::growing_plant_head_block::{self, GrowingPlantHead};
use super::prelude::*;

pub struct KelpBlock;

impl GrowingPlant for KelpBlock {
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
impl GrowingPlantHead for KelpBlock {
}

impl BlockBehaviour for KelpBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        growing_plant_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        growing_plant_head_block::update_shape(self, state, level, pos, direction, neighbour_state)
    });
}

register_block_class!("KelpBlock", |_| Box::new(KelpBlock));
