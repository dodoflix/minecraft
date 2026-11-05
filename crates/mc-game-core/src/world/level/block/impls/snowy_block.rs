//! `net.minecraft.world.level.block.SnowyBlock` (podzol; the base of grass block and mycelium).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

pub struct SnowyBlock;

/// `SnowyBlock.isSnowySetting`.
pub fn is_snowy_setting(above_state: BlockState) -> bool {
    above_state.is_tag("snow")
}

/// `SnowyBlock.updateShape`.
pub fn update_shape(state: BlockState, direction: Direction, neighbour_state: BlockState) -> BlockState {
    if direction == Direction::Up { state.set(props::SNOWY, is_snowy_setting(neighbour_state)) } else { state }
}

/// `SnowyBlock.getStateForPlacement`.
pub fn get_state_for_placement(block: Block, level: &ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
    let above_state = level.get_block_state(context.get_clicked_pos().above());
    Some(block.default_state().set(props::SNOWY, is_snowy_setting(above_state)))
}

impl BlockBehaviour for SnowyBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { update_shape(state, direction, neighbour_state) });
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        get_state_for_placement(block, level, context)
    }
}

register_block_class!("SnowyBlock", |_| Box::new(SnowyBlock));
