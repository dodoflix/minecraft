//! `net.minecraft.world.level.block.CocoaBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct CocoaBlock;

impl BlockBehaviour for CocoaBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.relative(state.get(props::HORIZONTAL_FACING))).is_tag("supports_cocoa")
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.random().next_int_bounded(5) == 0 {
            let age = state.get(props::AGE_2);
            if age < 2 {
                level.set_block_flags(pos, state.set(props::AGE_2, age + 1), 2);
            }
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction);
                if state.can_survive(&*level, pos) {
                    return Some(state);
                }
            }
        }
        None
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
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        state.get(props::AGE_2) < 2
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        level.set_block_flags(pos, state.set(props::AGE_2, state.get(props::AGE_2) + 1), 2);
    }
}

register_block_class!("CocoaBlock", |_| Box::new(CocoaBlock));
