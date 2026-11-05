//! `net.minecraft.world.level.block.CocoaBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;

pub struct CocoaBlock;

impl BlockBehaviour for CocoaBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.relative(state.get(props::HORIZONTAL_FACING))).is_tag("supports_cocoa")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CocoaBlock", |_| Box::new(CocoaBlock));
