//! `net.minecraft.world.level.block.GlazedTerracottaBlock` (extends `HorizontalDirectionalBlock`).
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct GlazedTerracottaBlock;

impl BlockBehaviour for GlazedTerracottaBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_opposite(),
        ))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("GlazedTerracottaBlock", |_| Box::new(GlazedTerracottaBlock));
