//! `net.minecraft.world.level.block.SpreadingSnowyBlock` (grass block, mycelium): dies under cover, spreads to dirt.
//! `MyceliumBlock` adds only client particles; `GrassBlock` (bone meal) lives in `grass_block.rs` and reuses
//! [`random_tick`] / [`can_stay_alive`].
use super::prelude::*;
use super::snowy_block;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

pub struct SpreadingSnowyBlock;

/// `SpreadingSnowyBlock.canStayAlive`.
pub fn can_stay_alive(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let above_state = level.get_block_state(pos.above());
    if above_state.is_block(Block::SNOW) && above_state.get(props::LAYERS) == 1 {
        true
    } else if above_state.get_fluid_state().amount() == 8 {
        false
    } else {
        let light_dampening_top_face = mc_light::get_light_dampening_into(state, above_state, Direction::Up, i32::from(above_state.get_light_block()));
        light_dampening_top_face < 15
    }
}

/// `SpreadingSnowyBlock.canPropagate`.
fn can_propagate(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    can_stay_alive(state, level, pos) && !level.get_fluid_state(pos.above()).is_tag("water")
}

/// `SpreadingSnowyBlock.randomTick` (the base block of grass and mycelium is dirt).
pub fn random_tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let base_block = Block::DIRT;
    if !can_stay_alive(state, &*level, pos) {
        level.set_block_and_update(pos, base_block.default_state());
    } else if level.get_max_local_raw_brightness(pos.above()) >= 9 {
        let default_block_state = state.block().default_state();
        for _ in 0..4 {
            let dx = level.random().next_int_bounded(3) - 1;
            let dy = level.random().next_int_bounded(5) - 3;
            let dz = level.random().next_int_bounded(3) - 1;
            let test_pos = pos.offset(dx, dy, dz);
            if level.get_block_state(test_pos).is_block(base_block) && can_propagate(default_block_state, &*level, test_pos) {
                let snowy = snowy_block::is_snowy_setting(level.get_block_state(test_pos.above()));
                level.set_block_and_update(test_pos, default_block_state.set(props::SNOWY, snowy));
            }
        }
    }
}

impl BlockBehaviour for SpreadingSnowyBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { snowy_block::update_shape(state, direction, neighbour_state) });
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        random_tick(state, level, pos);
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        snowy_block::get_state_for_placement(block, level, context)
    }
}

register_block_class!("MyceliumBlock", |_| Box::new(SpreadingSnowyBlock));
