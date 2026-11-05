//! `net.minecraft.world.level.block.CeilingHangingSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block;

pub struct CeilingHangingSignBlock;

impl BlockBehaviour for CeilingHangingSignBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.above()).is_face_sturdy_support(level, pos.above(), Direction::Down, SupportType::Center)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !self.can_survive(state, &*level, pos) { air() } else { sign_block::update_shape(state, level, pos) }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        state.set(props::ROTATION_16, rotation.rotate_steps(state.get(props::ROTATION_16), 16))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::ROTATION_16, mirror.mirror_steps(state.get(props::ROTATION_16), 16))
    }
}

register_block_class!("CeilingHangingSignBlock", |_| Box::new(CeilingHangingSignBlock));
