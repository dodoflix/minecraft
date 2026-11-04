//! `net.minecraft.world.level.block.SugarCaneBlock`.
use super::prelude::*;

pub struct SugarCaneBlock;

impl BlockBehaviour for SugarCaneBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let state_below = level.get_block_state(pos.below());
        if state_below.is_block(state.block()) {
            return true;
        }
        if state_below.is_tag("supports_sugar_cane") {
            let below = pos.below();
            for direction in Direction::HORIZONTAL {
                let block_state = level.get_block_state(below.relative(direction));
                let fluid_state = level.get_fluid_state(below.relative(direction));
                if fluid_state.is_tag("supports_sugar_cane_adjacently") || block_state.is_tag("supports_sugar_cane_adjacently") {
                    return true;
                }
            }
        }
        false
    }
}

register_block_class!("SugarCaneBlock", |_| Box::new(SugarCaneBlock));
