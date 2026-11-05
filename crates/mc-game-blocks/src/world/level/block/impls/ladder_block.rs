//! `net.minecraft.world.level.block.LadderBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct LadderBlock;

impl BlockBehaviour for LadderBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut mc_game_core::server::level::ServerLevel, context: &mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        if !helpers::replacing_clicked_on_block(level, context) {
            let state = level.get_block_state(pos.relative(context.get_clicked_face().get_opposite()));
            if state.is_block(block) && state.get(props::HORIZONTAL_FACING) == context.get_clicked_face() {
                return None;
            }
        }
        let mut state = block.default_state();
        let replaced_fluid_state = level.get_fluid_state(pos);
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, pos) {
                    return Some(state.set(props::WATERLOGGED, replaced_fluid_state.get_type() == mc_data::fluid::Fluid::WATER));
                }
            }
        }
        None
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = state.get(props::HORIZONTAL_FACING);
        let attach = pos.relative(direction.get_opposite());
        level.get_block_state(attach).is_face_sturdy_at(level, attach, direction)
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
        if direction_to_neighbour.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) {
            air()
        } else {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("LadderBlock", |_| Box::new(LadderBlock));
