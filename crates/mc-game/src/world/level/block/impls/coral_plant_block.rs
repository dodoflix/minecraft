//! `net.minecraft.world.level.block.CoralPlantBlock` (extends `BaseCoralPlantTypeBlock`).
use super::base_coral_plant_type_block::{self as base, dead_block};
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

pub struct CoralPlantBlock {
    dead_block: Block,
}

impl BlockBehaviour for CoralPlantBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(base::get_state_for_placement(block, level, context))
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        base::try_schedule_die_tick(state, level, pos);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !base::scan_for_water(state, &*level, pos) {
            level.set_block_flags(pos, self.dead_block.default_state().set(props::WATERLOGGED, false), 2);
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        base::can_survive(level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) {
            air()
        } else {
            base::try_schedule_die_tick(state, level, pos);
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            base::update_shape(state, level, pos, direction)
        }
    });
}

register_block_class!("CoralPlantBlock", |block| Box::new(CoralPlantBlock { dead_block: dead_block(block) }));
