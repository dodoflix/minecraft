//! `net.minecraft.world.level.block.CoralBlock`.
use mc_game_core::{register_block_class, update_shape};
use super::base_coral_plant_type_block::dead_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;

pub struct CoralBlock {
    dead_block: Block,
}

fn scan_for_water(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    Direction::ALL.into_iter().any(|direction| level.get_fluid_state(pos.relative(direction)).is_tag("water"))
}

impl BlockBehaviour for CoralBlock {
    fn tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !scan_for_water(&*level, pos) {
            level.set_block_flags(pos, self.dead_block.default_state(), 2);
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !scan_for_water(&*level, pos) {
            let delay = 60 + level.random().next_int_bounded(40);
            helpers::schedule_block_tick(level, pos, state.block(), delay);
        }
        state
    });
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        if !scan_for_water(&*level, pos) {
            let delay = 60 + level.random().next_int_bounded(40);
            helpers::schedule_block_tick(level, pos, block, delay);
        }
        Some(block.default_state())
    }
}

register_block_class!("CoralBlock", |block| Box::new(CoralBlock { dead_block: dead_block(block) }));
