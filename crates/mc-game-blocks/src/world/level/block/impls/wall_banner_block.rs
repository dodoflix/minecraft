//! `net.minecraft.world.level.block.WallBannerBlock` (extends `AbstractBannerBlock`).
use super::abstract_banner_block;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct WallBannerBlock;

impl BlockBehaviour for WallBannerBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
        true
    }
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        abstract_banner_block::get_clone_item_stack(state, level, pos)
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level
            .get_block_state(pos.relative(state.get(props::HORIZONTAL_FACING).get_opposite()))
            .is_solid()
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, pos) {
                    return Some(state);
                }
            }
        }
        None
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == state.get(props::HORIZONTAL_FACING).get_opposite()
                && !state.can_survive(&*level, pos)
            {
                air()
            } else {
                state
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("WallBannerBlock", |_| Box::new(WallBannerBlock));
