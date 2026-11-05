//! `net.minecraft.world.level.block.WallSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block::{self, sign_block_common};
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

pub struct WallSignBlock;

impl BlockBehaviour for WallSignBlock {
    crate::simple_waterlogged!();
    sign_block_common!();
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.relative(state.get(props::HORIZONTAL_FACING).get_opposite())).is_solid()
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        let waterlogged = sign_block::waterlogged_by(&*level, pos);
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, pos) {
                    return Some(state.set(props::WATERLOGGED, waterlogged));
                }
            }
        }
        None
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) {
            air()
        } else {
            sign_block::update_shape(state, level, pos)
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("WallSignBlock", |_| Box::new(WallSignBlock));
