//! `net.minecraft.world.level.block.SlabBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::simple_waterlogged;
use mc_data::block::PathComputationType;
use mc_data::enums::SlabType;
use mc_data::fluid::{Fluid, FluidState};

pub struct SlabBlock;

impl BlockBehaviour for SlabBlock {
    fn is_liquid_block_container(&self) -> bool {
        true
    }
    fn can_place_liquid(&self, user: Option<&dyn Entity>, level: &dyn BlockGetter, pos: BlockPos, state: BlockState, fluid: Fluid) -> bool {
        state.get(props::SLAB_TYPE) != SlabType::Double && simple_waterlogged::can_place_liquid(user, level, pos, state, fluid)
    }
    fn place_liquid(&self, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState, fluid_state: FluidState) -> bool {
        state.get(props::SLAB_TYPE) != SlabType::Double && simple_waterlogged::place_liquid(level, pos, state, fluid_state)
    }
    fn is_bucket_pickup(&self) -> bool {
        true
    }
    fn pickup_block(&self, user: Option<&dyn Entity>, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState) -> mc_game_core::world::item::ItemStack {
        simple_waterlogged::pickup_block(user, level, pos, state)
    }
    fn get_pickup_sound(&self) -> Option<mc_data::SoundEvent> {
        simple_waterlogged::get_pickup_sound()
    }
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        _direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    }
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        match ty {
            PathComputationType::Water => state.get_fluid_state().is_tag("water"),
            _ => false,
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_block_state = level.get_block_state(pos);
        if replaced_block_state.is_block(block) {
            Some(replaced_block_state.set(props::SLAB_TYPE, SlabType::Double).set(props::WATERLOGGED, false))
        } else {
            let replaced_fluid_state = level.get_fluid_state(pos);
            let result = block.default_state().set(props::SLAB_TYPE, SlabType::Bottom).set(props::WATERLOGGED, replaced_fluid_state.get_type() == Fluid::WATER);
            let clicked_face = context.get_clicked_face();
            if clicked_face != Direction::Down && (clicked_face == Direction::Up || context.use_on.get_click_location().y - pos.y() as f64 <= 0.5) {
                Some(result)
            } else {
                Some(result.set(props::SLAB_TYPE, SlabType::Top))
            }
        }
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let item_stack = context.use_on.get_item_in_hand();
        let ty = state.get(props::SLAB_TYPE);
        if ty == SlabType::Double || item_stack.is_empty() || Some(item_stack.item()) != state.block().item() {
            false
        } else if context.replacing_clicked_on_block(false) {
            let above = context.use_on.get_click_location().y - context.get_clicked_pos().y() as f64 > 0.5;
            let clicked_face = context.get_clicked_face();
            if ty == SlabType::Bottom {
                clicked_face == Direction::Up || (above && clicked_face.get_axis().is_horizontal())
            } else {
                clicked_face == Direction::Down || (!above && clicked_face.get_axis().is_horizontal())
            }
        } else {
            true
        }
    }
}

register_block_class!("SlabBlock", |_| Box::new(SlabBlock));
