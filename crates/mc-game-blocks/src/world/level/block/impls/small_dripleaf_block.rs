//! `net.minecraft.world.level.block.SmallDripleafBlock` (extends `DoublePlantBlock`).
use mc_data::enums::DoubleBlockHalf;
use mc_data::fluid::Fluid;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::double_plant_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::{register_block_class, update_shape};

pub struct SmallDripleafBlock;

/// `SmallDripleafBlock.mayPlaceOn` (`super.mayPlaceOn` is the `SUPPORTS_VEGETATION` default).
struct DefaultVegetation;
impl MayPlaceOn for DefaultVegetation {}

impl MayPlaceOn for SmallDripleafBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_tag("supports_small_dripleaf")
            || (level
                .get_fluid_state(pos.above())
                .is_source_of_type(Fluid::WATER)
                && DefaultVegetation.may_place_on(state, level, pos))
    }
}

impl BlockBehaviour for SmallDripleafBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        // `DoublePlantBlock.getStateForPlacement`
        let pos = context.get_clicked_pos();
        if !(pos.y() < level.get_max_y()
            && level
                .get_block_state(pos.above())
                .can_be_replaced_by(context))
        {
            return None;
        }
        let state = block.default_state().set(
            props::HORIZONTAL_FACING,
            context.use_on.get_horizontal_direction().get_opposite(),
        );
        Some(double_plant_block::copy_waterlogged_from(
            &*level, pos, state,
        ))
    }
    fn set_placed_by(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _placer: Option<&mut mc_game_core::world::entity::ServerPlayer>,
        _stack: &mc_game_core::world::item::ItemStack,
    ) {
        let above_pos = pos.above();
        let upper = state
            .block()
            .default_state()
            .set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper)
            .set(
                props::HORIZONTAL_FACING,
                state.get(props::HORIZONTAL_FACING),
            );
        let block_state = double_plant_block::copy_waterlogged_from(&*level, above_pos, upper);
        level.set_block_and_update(above_pos, block_state);
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        _level: &dyn LevelReader,
        _pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        true
    }
    #[expect(
        clippy::only_used_in_recursion,
        reason = "mirrors SmallDripleafBlock.performBonemeal, which forwards the source when growing from the upper half"
    )]
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        source: BonemealSource,
    ) {
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower {
            let above = pos.above();
            let legacy = level.get_fluid_state(above).create_legacy_block();
            level.set_block_flags(above, legacy, 18);
            super::big_dripleaf_block::place_with_random_height(
                level,
                pos,
                state.get(props::HORIZONTAL_FACING),
            );
        } else {
            let below_pos = pos.below();
            let below_state = level.get_block_state(below_pos);
            self.perform_bonemeal(level, below_pos, below_state, source);
        }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper {
            double_plant_block::can_survive(self, state, level, pos)
        } else {
            let below_pos = pos.below();
            self.may_place_on(level.get_block_state(below_pos), level, below_pos)
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            double_plant_block::update_shape(state, &*level, pos, direction, neighbour_state)
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("SmallDripleafBlock", |_| Box::new(SmallDripleafBlock));
