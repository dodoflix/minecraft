//! `net.minecraft.world.level.block.ComparatorBlock` (extends `DiodeBlock`).
use super::diode_block;
use super::prelude::*;

pub struct ComparatorBlock;

impl BlockBehaviour for ComparatorBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        diode_block::can_survive_on(level, below_pos, level.get_block_state(below_pos))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !diode_block::can_survive_on(&*level, neighbour_pos, neighbour_state) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn get_direct_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        state.get_signal(level, pos, direction)
    }
    fn get_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::HORIZONTAL_FACING) == direction { self.own_signal(state, level, pos) } else { 0 }
    }
    fn own_signal(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        // `getOutputSignal` reads the `ComparatorBlockEntity`; without a block entity it is 0 (known gap in STATUS).
        0
    }
}

register_block_class!("ComparatorBlock", |_| Box::new(ComparatorBlock));
