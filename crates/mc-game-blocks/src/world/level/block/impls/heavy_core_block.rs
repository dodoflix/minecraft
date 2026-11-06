//! `net.minecraft.world.level.block.HeavyCoreBlock` (`SimpleWaterloggedBlock`).
use mc_data::block::PathComputationType;
use mc_data::fluid::Fluid;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct HeavyCoreBlock;

impl BlockBehaviour for HeavyCoreBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::WATERLOGGED,
            level.get_fluid_state(context.get_clicked_pos()).get_type() == Fluid::WATER,
        ))
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );
}

register_block_class!("HeavyCoreBlock", |_| Box::new(HeavyCoreBlock));
