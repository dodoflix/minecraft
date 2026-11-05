//! `net.minecraft.world.level.block.StandingSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block;

pub struct StandingSignBlock;

impl BlockBehaviour for StandingSignBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !self.can_survive(state, &*level, pos) { air() } else { sign_block::update_shape(state, level, pos) }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        state.set(props::ROTATION_16, rotation.rotate_steps(state.get(props::ROTATION_16), 16))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::ROTATION_16, mirror.mirror_steps(state.get(props::ROTATION_16), 16))
    }
}

register_block_class!("StandingSignBlock", |_| Box::new(StandingSignBlock));
