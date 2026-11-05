//! `net.minecraft.world.level.block.EndPortalFrameBlock`: placement, analog signal, `rotate` / `mirror` and the
//! stronghold portal pattern (`getOrCreatePortalShape`, used by `EnderEyeItem`).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_data::block::PathComputationType;
use mc_game_core::world::level::block::pattern::{BlockInWorld, BlockPattern, BlockPatternBuilder};
use std::sync::LazyLock;

pub struct EndPortalFrameBlock;

impl BlockBehaviour for EndPortalFrameBlock {
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite()).set(props::EYE, false))
    }
    fn get_analog_output_signal(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
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

/// `BlockInWorld.hasState(BlockStatePredicate.forBlock(END_PORTAL_FRAME).where(HAS_EYE, true).where(FACING, facing))`.
fn frame_with_eye_facing(facing: Direction) -> impl Fn(&mut BlockInWorld, &dyn LevelReader) -> bool + Send + Sync + 'static {
    move |block, level| {
        block.get_state(level).is_some_and(|s| s.is_block(Block::END_PORTAL_FRAME) && s.get(props::EYE) && s.get(props::HORIZONTAL_FACING) == facing)
    }
}

/// `EndPortalFrameBlock.getOrCreatePortalShape()`.
pub fn get_or_create_portal_shape() -> &'static BlockPattern {
    static PORTAL_SHAPE: LazyLock<BlockPattern> = LazyLock::new(|| {
        BlockPatternBuilder::start()
            .aisle(&["?vvv?", ">???<", ">???<", ">???<", "?^^^?"])
            .where_('?', |block, level| block.get_state(level).is_some())
            .where_('^', frame_with_eye_facing(Direction::South))
            .where_('>', frame_with_eye_facing(Direction::West))
            .where_('v', frame_with_eye_facing(Direction::North))
            .where_('<', frame_with_eye_facing(Direction::East))
            .build()
    });
    &PORTAL_SHAPE
}

register_block_class!("EndPortalFrameBlock", |_| Box::new(EndPortalFrameBlock));
