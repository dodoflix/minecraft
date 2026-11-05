//! `net.minecraft.world.level.block.ObserverBlock` (extends `DirectionalBlock`).
use super::prelude::*;

pub struct ObserverBlock;

impl BlockBehaviour for ObserverBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        // `startSignal`: `!level.isClientSide()` always holds on the server
        if state.get(props::FACING) == direction && !state.get(props::POWERED) && !level.has_scheduled_block_tick(pos, state.block()) {
            helpers::schedule_block_tick(level, pos, state.block(), 2);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
    fn should_redstone_wire_connect_to(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Option<Direction>) -> bool {
        direction == Some(state.get(props::FACING))
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
}

register_block_class!("ObserverBlock", |_| Box::new(ObserverBlock));
