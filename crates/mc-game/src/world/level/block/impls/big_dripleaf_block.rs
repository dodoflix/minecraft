//! `net.minecraft.world.level.block.BigDripleafBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;

pub struct BigDripleafBlock;

impl BlockBehaviour for BigDripleafBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        below_state.is_block(state.block()) || below_state.is_block(Block::BIG_DRIPLEAF_STEM) || below_state.is_tag("supports_big_dripleaf")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) {
            air()
        } else {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            if direction == Direction::Up && neighbour_state.is_block(state.block()) {
                Block::BIG_DRIPLEAF_STEM.default_state().with_properties_of(state)
            } else {
                state
            }
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BigDripleafBlock", |_| Box::new(BigDripleafBlock));
