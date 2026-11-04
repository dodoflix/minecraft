//! `net.minecraft.world.level.block.AmethystClusterBlock` (extends `AmethystBlock`).
use super::prelude::*;

pub struct AmethystClusterBlock;

impl BlockBehaviour for AmethystClusterBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = state.get(props::FACING);
        let adjacent_pos = pos.relative(direction.get_opposite());
        level.get_block_state(adjacent_pos).is_face_sturdy_at(level, adjacent_pos, direction)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction == state.get(props::FACING).get_opposite() && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("AmethystClusterBlock", |_| Box::new(AmethystClusterBlock));
