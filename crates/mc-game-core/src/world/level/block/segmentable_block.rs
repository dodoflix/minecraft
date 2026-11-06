//! `SegmentableBlock` (`FlowerBedBlock`, `LeafLitterBlock`): 1..=4 segments per block, placing more of the same item adds
//! a segment. Free functions taking the implementor's facing/amount properties.

use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::direction::Direction;
use mc_data::block::{Block, BlockState};
use mc_data::props::{EnumProperty, IntegerProperty};

/// `SegmentableBlock.MIN_SEGMENT` / `MAX_SEGMENT`.
pub const MIN_SEGMENT: i32 = 1;
pub const MAX_SEGMENT: i32 = 4;

/// `SegmentableBlock.canBeReplaced(state, context, segment)`.
pub fn can_be_replaced(
    state: BlockState,
    context: &BlockPlaceContext,
    segment: IntegerProperty,
) -> bool {
    let held = context.use_on.get_item_in_hand();
    !context.is_secondary_use_active()
        && !held.is_empty()
        && Some(held.item()) == state.block().item()
        && state.get(segment) < 4
}

/// `SegmentableBlock.getStateForPlacement(context, block, segment, facing)`.
pub fn get_state_for_placement(
    level: &ServerLevel,
    context: &BlockPlaceContext,
    block: Block,
    segment: IntegerProperty,
    facing: EnumProperty<Direction>,
) -> BlockState {
    let state = level.get_block_state(context.get_clicked_pos());
    if state.is_block(block) {
        state.set(segment, (state.get(segment) + 1).min(4))
    } else {
        block
            .default_state()
            .set(facing, context.get_horizontal_direction().get_opposite())
    }
}
