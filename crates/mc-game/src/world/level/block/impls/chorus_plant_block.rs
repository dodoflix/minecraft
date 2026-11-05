//! `net.minecraft.world.level.block.ChorusPlantBlock` (extends `PipeBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;

pub struct ChorusPlantBlock;

/// `ChorusPlantBlock.getStateWithConnections`.
pub fn get_state_with_connections(level: &dyn BlockGetter, pos: BlockPos, default_state: BlockState) -> BlockState {
    let down = level.get_block_state(pos.below());
    let up = level.get_block_state(pos.above());
    let north = level.get_block_state(pos.north());
    let east = level.get_block_state(pos.east());
    let south = level.get_block_state(pos.south());
    let west = level.get_block_state(pos.west());
    let block = default_state.block();
    let flower = |s: BlockState| s.is_block(block) || s.is_block(Block::CHORUS_FLOWER);
    default_state
        .try_set(props::DOWN, flower(down) || down.is_tag("supports_chorus_plant"))
        .unwrap_or(default_state)
        .set(props::UP, flower(up))
        .set(props::NORTH, flower(north))
        .set(props::EAST, flower(east))
        .set(props::SOUTH, flower(south))
        .set(props::WEST, flower(west))
}

impl BlockBehaviour for ChorusPlantBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        Some(get_state_with_connections(&*level, context.get_clicked_pos(), block.default_state()))
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        let block_above_or_below = !level.get_block_state(pos.above()).is_air() && !below_state.is_air();
        for direction in Direction::HORIZONTAL {
            let neighbor_pos = pos.relative(direction);
            let neighbor_state = level.get_block_state(neighbor_pos);
            if neighbor_state.is_block(state.block()) {
                if block_above_or_below {
                    return false;
                }
                let below = level.get_block_state(neighbor_pos.below());
                if below.is_block(state.block()) || below.is_tag("supports_chorus_plant") {
                    return true;
                }
            }
        }
        below_state.is_block(state.block()) || below_state.is_tag("supports_chorus_plant")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
            state
        } else {
            let connect = neighbour_state.is_block(state.block())
                || neighbour_state.is_block(Block::CHORUS_FLOWER)
                || (direction == Direction::Down && neighbour_state.is_tag("supports_chorus_plant"));
            state.set(helpers::pipe_property(direction), connect)
        }
    });
}

register_block_class!("ChorusPlantBlock", |_| Box::new(ChorusPlantBlock));
