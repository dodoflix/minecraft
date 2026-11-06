//! `net.minecraft.world.level.block.WallSkullBlock` (extends `AbstractSkullBlock`); also the player/piglin/wither wall heads.
use super::abstract_skull_block;
use mc_data::block::PathComputationType;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::Orientation;

pub struct WallSkullBlock;

/// `WallSkullBlock.getStateForPlacement`.
pub fn get_state_for_placement(
    block: Block,
    level: &mut ServerLevel,
    context: &BlockPlaceContext,
) -> Option<BlockState> {
    let pos = context.get_clicked_pos();
    let mut state = abstract_skull_block::get_state_for_placement(block, &*level, pos);
    for direction in context.get_nearest_looking_directions() {
        if direction.get_axis().is_horizontal() {
            state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
            if !level
                .get_block_state(pos.relative(direction))
                .can_be_replaced_by(context)
            {
                return Some(state);
            }
        }
    }
    None
}

impl BlockBehaviour for WallSkullBlock {
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        get_state_for_placement(block, level, context)
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        abstract_skull_block::neighbor_changed(state, level, pos);
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("WallSkullBlock", |_| Box::new(WallSkullBlock));
