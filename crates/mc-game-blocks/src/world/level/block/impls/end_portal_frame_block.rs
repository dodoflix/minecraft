//! `net.minecraft.world.level.block.EndPortalFrameBlock`: placement, analog signal, `rotate` / `mirror`.
use mc_data::block::PathComputationType;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct EndPortalFrameBlock;

impl BlockBehaviour for EndPortalFrameBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(
            block
                .default_state()
                .set(
                    props::HORIZONTAL_FACING,
                    context.get_horizontal_direction().get_opposite(),
                )
                .set(props::EYE, false),
        )
    }
    fn get_analog_output_signal(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        if state.get(props::EYE) { 15 } else { 0 }
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("EndPortalFrameBlock", |_| Box::new(EndPortalFrameBlock));
