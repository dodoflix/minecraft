//! `net.minecraft.world.level.block.SegmentableBlock` (leaf litter, pink petals, wildflowers).
use mc_data::props::{EnumProperty, IntegerProperty};
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;

/// `SegmentableBlock.MAX_SEGMENT`.
const MAX_SEGMENT: i32 = 4;

/// `SegmentableBlock.canBeReplaced(state, context, segment)`: another segment of the same block can be added.
pub fn can_be_replaced(
    state: BlockState,
    context: &BlockPlaceContext,
    segment: IntegerProperty,
) -> bool {
    let held = context.use_on.get_item_in_hand();
    !context.is_secondary_use_active()
        && !held.is_empty()
        && Some(held.item()) == state.block().item()
        && state.get(segment) < MAX_SEGMENT
}

/// `SegmentableBlock.getStateForPlacement(context, block, segment, facing)`.
pub fn get_state_for_placement(
    level: &dyn BlockGetter,
    context: &BlockPlaceContext,
    block: Block,
    segment: IntegerProperty,
    facing: EnumProperty<Direction>,
) -> BlockState {
    let state = level.get_block_state(context.get_clicked_pos());
    if state.is_block(block) {
        state.set(segment, MAX_SEGMENT.min(state.get(segment) + 1))
    } else {
        block
            .default_state()
            .set(facing, context.get_horizontal_direction().get_opposite())
    }
}
