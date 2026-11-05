//! `net.minecraft.world.level.block.BigDripleafStemBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;

pub struct BigDripleafStemBlock;

impl BlockBehaviour for BigDripleafStemBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        let above_state = level.get_block_state(pos.above());
        (below_state.is_block(state.block()) || below_state.is_tag("supports_big_dripleaf"))
            && (above_state.is_block(state.block()) || above_state.is_block(Block::BIG_DRIPLEAF))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if (direction == Direction::Down || direction == Direction::Up) && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BigDripleafStemBlock", |_| Box::new(BigDripleafStemBlock));
