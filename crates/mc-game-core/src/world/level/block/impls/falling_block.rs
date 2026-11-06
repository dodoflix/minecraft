//! `net.minecraft.world.level.block.FallingBlock` (sand, gravel, concrete powder, anvil, dragon egg).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::falling_block_entity::FallingBlockEntity;
use crate::world::level::block::behaviour;

pub struct FallingBlock;

/// `FallingBlock.isFree(state)`.
pub fn is_free(state: BlockState) -> bool {
    state.is_air() || state.is_tag("fire") || state.liquid() || state.can_be_replaced()
}

/// `FallingBlock.updateShape` (`delay` = `getDelayAfterPlace()`).
pub fn update_shape(
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    delay: i32,
) -> BlockState {
    helpers::schedule_block_tick(level, pos, state.block(), delay);
    state
}

/// `FallingBlock.onPlace`.
pub fn on_place(state: BlockState, level: &mut ServerLevel, pos: BlockPos, delay: i32) {
    helpers::schedule_block_tick(level, pos, state.block(), delay);
}

/// `FallingBlock.tick`: the block starts to fall when nothing carries it.
pub fn tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if is_free(level.get_block_state(pos.below())) && pos.y() >= level.get_min_y() {
        FallingBlockEntity::fall_with(level, pos, state, |entity| {
            behaviour(state.block()).falling(entity)
        });
    }
}

impl BlockBehaviour for FallingBlock {
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            update_shape(state, level, pos, 2)
        }
    );
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        on_place(state, level, pos, 2);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        tick(state, level, pos);
    }
}

register_block_class!("FallingBlock", |_| Box::new(FallingBlock));
