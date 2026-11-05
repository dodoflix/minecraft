//! `net.minecraft.world.level.block.LiquidBlock`.
use super::prelude::*;
use crate::world::level::material::fluid_behaviour;

pub struct LiquidBlock;

/// `LiquidBlock.shouldBubbleColumnOccupy`.
fn should_bubble_column_occupy(state: BlockState) -> bool {
    let fluid = state.get_fluid_state();
    fluid.is_tag("bubble_column_can_occupy") && fluid.is_source() && fluid.is_full()
}

impl BlockBehaviour for LiquidBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get_fluid_state().is_source() || neighbour_state.get_fluid_state().is_source() {
            // `this.fluid.getTickDelay(level)`: the block's own fluid (water / lava)
            let fluid = if state.block() == Block::LAVA { mc_data::fluid::Fluid::LAVA } else { mc_data::fluid::Fluid::WATER };
            let delay = fluid_behaviour(fluid).get_tick_delay(&*level);
            level.schedule_fluid_tick(pos, state.get_fluid_state().get_type(), delay, mc_world::ticks::TickPriority::Normal);
        }
        if direction == Direction::Down && should_bubble_column_occupy(state) {
            // `tryScheduleBubbleBlockColumn`
            if neighbour_state.is_tag("enables_bubble_column_drag_down") || neighbour_state.is_tag("enables_bubble_column_push_up") {
                helpers::schedule_block_tick(level, pos, state.block(), 20);
            }
        }
        state
    });
}

register_block_class!("LiquidBlock", |_| Box::new(LiquidBlock));
