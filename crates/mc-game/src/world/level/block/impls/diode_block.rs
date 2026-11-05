//! `net.minecraft.world.level.block.DiodeBlock` (extends `HorizontalDirectionalBlock`; base of repeater / comparator).
use super::prelude::*;

pub struct DiodeBlock;

/// `DiodeBlock.isDiode`.
pub fn is_diode(state: BlockState) -> bool {
    helpers::is_instance_of(state, "DiodeBlock")
}

/// `DiodeBlock.canSurviveOn`.
pub fn can_survive_on(level: &dyn BlockGetter, neighbour_pos: BlockPos, neighbour_state: BlockState) -> bool {
    neighbour_state.is_face_sturdy_support(level, neighbour_pos, Direction::Up, SupportType::Rigid)
}

impl BlockBehaviour for DiodeBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        can_survive_on(level, below_pos, level.get_block_state(below_pos))
    }
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
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        // `getOutputSignal` is 15 for repeaters; the comparator's block entity part is added with its block entity.
        if state.get(props::POWERED) { 15 } else { 0 }
    }
}

register_block_class!("DiodeBlock", |_| Box::new(DiodeBlock));
