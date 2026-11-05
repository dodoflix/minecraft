//! `net.minecraft.world.level.block.PathBlock` (dirt path; the base block is dirt).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::item::BlockPlaceContext;
use crate::world::level::gameevent::GameEventContext;
use mc_data::GameEvent;

pub struct PathBlock;

/// `PathBlock.turnToBaseBlock(sourceEntity, state, level, pos)`.
pub fn turn_to_base_block(source_entity: Option<EntityId>, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let new_state = helpers::push_entities_up(level, state, Block::DIRT.default_state(), pos);
    level.set_block_and_update(pos, new_state);
    level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of(source_entity, Some(new_state)));
}

impl BlockBehaviour for PathBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = level.get_block_state(pos.above());
        !above_state.is_solid() || helpers::is_instance_of(above_state, "FenceGateBlock")
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        if !block.default_state().can_survive(&*level, pos) {
            Some(helpers::push_entities_up(level, block.default_state(), Block::DIRT.default_state(), pos))
        } else {
            Some(block.default_state())
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        turn_to_base_block(None, state, level, pos);
    }
}

register_block_class!("PathBlock", |_| Box::new(PathBlock));
