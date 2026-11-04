//! `net.minecraft.world.level.block.BubbleColumnBlock`.
use super::prelude::*;

pub struct BubbleColumnBlock;

/// `BubbleColumnBlock.canOccupy`.
fn can_occupy(bubble_column: Block, occupy_state: BlockState) -> bool {
    if occupy_state.is_block(bubble_column) {
        true
    } else {
        let occupy_fluid = occupy_state.get_fluid_state();
        occupy_fluid.is_tag("bubble_column_can_occupy")
            && helpers::is_instance_of(occupy_state, "LiquidBlock")
            && occupy_fluid.is_source()
            && occupy_fluid.amount() >= 8
    }
}

impl BlockBehaviour for BubbleColumnBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        below_state.is_block(state.block()) || below_state.is_tag("enables_bubble_column_push_up") || below_state.is_tag("enables_bubble_column_drag_down")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        helpers::schedule_water_tick(level, pos);
        if !state.can_survive(&*level, pos)
            || direction == Direction::Down
            || (direction == Direction::Up && !neighbour_state.is_block(state.block()) && can_occupy(state.block(), neighbour_state))
        {
            helpers::schedule_block_tick(level, pos, state.block(), 5);
        }
        state
    });
}

register_block_class!("BubbleColumnBlock", |_| Box::new(BubbleColumnBlock));
