//! `net.minecraft.world.level.block.FlowerBlock`-like `FlowerBedBlock` (pink petals, wildflowers; extends
//! `VegetationBlock`, implements `SegmentableBlock`).
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_game_core::world::level::block::PlainBlock;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::{register_block_class, update_shape};

pub struct FlowerBedBlock;
impl MayPlaceOn for FlowerBedBlock {}

impl BlockBehaviour for FlowerBedBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        mc_game_core::world::level::block::impls::vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            mc_game_core::world::level::block::impls::vegetation_block::update_shape(
                state, &*level, pos,
            )
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        super::segmentable_block::can_be_replaced(state, context, props::FLOWER_AMOUNT)
            || PlainBlock.can_be_replaced(state, context)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(super::segmentable_block::get_state_for_placement(
            &*level,
            context,
            block,
            props::FLOWER_AMOUNT,
            props::HORIZONTAL_FACING,
        ))
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
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) {
        let current_amount = state.get(props::FLOWER_AMOUNT);
        if current_amount < 4 {
            level.set_block_flags(pos, state.set(props::FLOWER_AMOUNT, current_amount + 1), 2);
        } else if let Some(item) = state.block().item() {
            helpers::pop_resource(level, pos, ItemStack::new(item));
        }
    }
}

register_block_class!("FlowerBedBlock", |_| Box::new(FlowerBedBlock));
