//! `net.minecraft.world.level.block.ConduitBlock` (a `BaseEntityBlock`: the block entity is `ConduitBlockEntity`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_data::block::PathComputationType;

pub struct ConduitBlock;

impl BlockBehaviour for ConduitBlock {
    mc_game_core::simple_waterlogged!();
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let fluid_state = level.get_fluid_state(context.get_clicked_pos());
        Some(block.default_state().set(props::WATERLOGGED, fluid_state.is_tag("water") && fluid_state.is_full()))
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
}

register_block_class!("ConduitBlock", |_| Box::new(ConduitBlock));
