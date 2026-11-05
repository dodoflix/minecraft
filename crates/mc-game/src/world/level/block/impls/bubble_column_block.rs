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
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut crate::server::level::ServerLevel,
        pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        is_precise: bool,
    ) {
        if is_precise {
            let state_above = level.get_block_state(pos.above());
            let nothing_above = state_above.get_collision_shape(&*level, pos).is_empty() && state_above.get_fluid_state().is_empty();
            let drag_down = state.get(props::DRAG);
            if nothing_above {
                entity.on_above_bubble_column(level, drag_down, pos);
            } else {
                entity.on_inside_bubble_column(level, drag_down);
            }
        }
    }
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
