//! `net.minecraft.world.level.block.ShelfMushroomBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;

pub struct ShelfMushroomBlock;

impl BlockBehaviour for ShelfMushroomBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let facing = state.get(props::HORIZONTAL_FACING);
        let support_pos = pos.relative(facing.get_opposite());
        level.get_block_state(support_pos).is_face_sturdy_at(level, support_pos, facing)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == state.get(props::HORIZONTAL_FACING).get_opposite() && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ShelfMushroomBlock", |_| Box::new(ShelfMushroomBlock));
