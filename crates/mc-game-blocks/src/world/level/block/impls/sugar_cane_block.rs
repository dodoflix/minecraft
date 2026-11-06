//! `net.minecraft.world.level.block.SugarCaneBlock`.
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct SugarCaneBlock;

impl BlockBehaviour for SugarCaneBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if level.is_empty_block(pos.above()) {
            let mut height = 1;
            while level
                .get_block_state(pos.below_n(height))
                .is_block(state.block())
            {
                height += 1;
            }
            if height < 3 {
                let age = state.get(props::AGE_15);
                if age == 15 {
                    level.set_block_and_update(pos.above(), state.block().default_state());
                    level.set_block_flags(pos, state.set(props::AGE_15, 0), 260);
                } else {
                    level.set_block_flags(pos, state.set(props::AGE_15, age + 1), 260);
                }
            }
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if !state.can_survive(&*level, pos) {
                helpers::schedule_block_tick(level, pos, state.block(), 1);
            }
            state
        }
    );
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
                if fluid_state.is_tag("supports_sugar_cane_adjacently")
                    || block_state.is_tag("supports_sugar_cane_adjacently")
                {
                    return true;
                }
            }
        }
        false
    }
}

register_block_class!("SugarCaneBlock", |_| Box::new(SugarCaneBlock));
