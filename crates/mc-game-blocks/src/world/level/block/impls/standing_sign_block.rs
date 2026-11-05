//! `net.minecraft.world.level.block.StandingSignBlock` (extends `SignBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::sign_block::{self, sign_block_common};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::rotation_segment;

pub struct StandingSignBlock;

impl BlockBehaviour for StandingSignBlock {
    mc_game_core::simple_waterlogged!();
    sign_block_common!();
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let waterlogged = sign_block::waterlogged_by(&*level, context.get_clicked_pos());
        Some(
            block
                .default_state()
                .set(props::ROTATION_16, rotation_segment::convert_to_segment(context.get_rotation() + 180.0))
                .set(props::WATERLOGGED, waterlogged),
        )
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
