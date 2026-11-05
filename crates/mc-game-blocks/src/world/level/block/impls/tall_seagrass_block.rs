//! `net.minecraft.world.level.block.TallSeagrassBlock` (extends `DoublePlantBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::double_plant_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_data::enums::DoubleBlockHalf;

pub struct TallSeagrassBlock;

impl MayPlaceOn for TallSeagrassBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_face_sturdy_at(level, pos, Direction::Up) && !state.is_tag("cannot_support_seagrass")
    }
}

impl BlockBehaviour for TallSeagrassBlock {
    fn is_liquid_block_container(&self) -> bool {
        true
    }
    fn can_place_liquid(&self, _user: Option<&dyn mc_game_core::world::entity::Entity>, _level: &dyn BlockGetter, _pos: BlockPos, _state: BlockState, _fluid: mc_data::fluid::Fluid) -> bool {
        false
    }
    fn place_liquid(&self, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState, _fluid_state: mc_data::fluid::FluidState) -> bool {
        false
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::new(mc_data::Item::SEAGRASS)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let state = double_plant_block::get_state_for_placement(block, level, context)?;
        let fluid_state = level.get_fluid_state(context.get_clicked_pos().above());
        if fluid_state.is_tag("water") && fluid_state.is_full() { Some(state) } else { None }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper {
            let below_state = level.get_block_state(pos.below());
            below_state.is_block(state.block()) && below_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
        } else {
            let fluid_state = level.get_fluid_state(pos);
            double_plant_block::can_survive(self, state, level, pos) && fluid_state.is_tag("water") && fluid_state.is_full()
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        double_plant_block::update_shape(state, &*level, pos, direction, neighbour_state)
    });
    fn set_placed_by(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _placer: Option<&mut mc_game_core::world::entity::ServerPlayer>, _stack: &ItemStack) {
        double_plant_block::set_placed_by(state.block(), level, pos);
    }
    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut mc_game_core::world::entity::ServerPlayer) -> BlockState {
        double_plant_block::player_will_destroy(level, pos, state, player)
    }
    fn player_destroy(
        &self,
        level: &mut ServerLevel,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        pos: BlockPos,
        _state: BlockState,
        block_entity: Option<&dyn mc_game_core::world::block_entity::BlockEntity>,
        destroyed_with: &ItemStack,
    ) {
        double_plant_block::player_destroy(level, player, pos, block_entity, destroyed_with);
    }
    fn get_seed(&self, state: BlockState, pos: BlockPos) -> i64 {
        double_plant_block::get_seed(state, pos)
    }
}

register_block_class!("TallSeagrassBlock", |_| Box::new(TallSeagrassBlock));
