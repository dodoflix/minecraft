//! `net.minecraft.world.level.block.LanternBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct LanternBlock;

/// `LanternBlock.getConnectedDirection`.
fn get_connected_direction(state: BlockState) -> Direction {
    if state.get(props::HANGING) {
        Direction::Down
    } else {
        Direction::Up
    }
}

impl BlockBehaviour for LanternBlock {
    mc_game_core::simple_waterlogged!();
    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut mc_game_core::server::level::ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_fluid_state = level.get_fluid_state(pos);
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis() == Axis::Y {
                let state = block
                    .default_state()
                    .set(props::HANGING, direction == Direction::Up);
                if state.can_survive(&*level, pos) {
                    return Some(state.set(
                        props::WATERLOGGED,
                        replaced_fluid_state.get_type() == mc_data::fluid::Fluid::WATER,
                    ));
                }
            }
        }
        None
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = get_connected_direction(state).get_opposite();
        helpers::can_support_center(level, pos.relative(direction), direction.get_opposite())
    }
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if get_connected_direction(state).get_opposite() == direction_to_neighbour
            && !state.can_survive(&*level, pos)
        {
            air()
        } else {
            state
        }
    }
}

register_block_class!("LanternBlock", |_| Box::new(LanternBlock));
