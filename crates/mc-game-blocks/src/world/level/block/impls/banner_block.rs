//! `net.minecraft.world.level.block.BannerBlock` (extends `AbstractBannerBlock`).
use mc_game_core::{register_block_class, update_shape};
use super::abstract_banner_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_game_core::world::level::block::rotation_segment;

pub struct BannerBlock;

impl BlockBehaviour for BannerBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
        true
    }
    fn get_clone_item_stack(&self, state: BlockState, level: &ServerLevel, pos: BlockPos, _include_data: bool) -> ItemStack {
        abstract_banner_block::get_clone_item_stack(state, level, pos)
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::ROTATION_16, rotation_segment::convert_to_segment(context.get_rotation() + 180.0)))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        state.set(props::ROTATION_16, rotation.rotate_steps(state.get(props::ROTATION_16), 16))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::ROTATION_16, mirror.mirror_steps(state.get(props::ROTATION_16), 16))
    }
}

register_block_class!("BannerBlock", |_| Box::new(BannerBlock));
