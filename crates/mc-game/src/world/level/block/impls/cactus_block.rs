//! `net.minecraft.world.level.block.CactusBlock`.
use super::prelude::*;

pub struct CactusBlock;

impl BlockBehaviour for CactusBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        for direction in Direction::HORIZONTAL {
            let neighbor = level.get_block_state(pos.relative(direction));
            if neighbor.is_solid() || level.get_fluid_state(pos.relative(direction)).is_tag("lava") {
                return false;
            }
        }
        let below_state = level.get_block_state(pos.below());
        (below_state.is_block(state.block()) || below_state.is_tag("supports_cactus")) && !level.get_block_state(pos.above()).liquid()
    }
}

register_block_class!("CactusBlock", |_| Box::new(CactusBlock));
