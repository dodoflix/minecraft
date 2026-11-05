//! `net.minecraft.world.level.block.TripWireHookBlock`.
use super::prelude::*;

pub struct TripWireHookBlock;

impl BlockBehaviour for TripWireHookBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = state.get(props::HORIZONTAL_FACING);
        let relative = pos.relative(direction.get_opposite());
        let block_state = level.get_block_state(relative);
        direction.get_axis().is_horizontal() && block_state.is_face_sturdy_at(level, relative, direction)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("TripWireHookBlock", |_| Box::new(TripWireHookBlock));
