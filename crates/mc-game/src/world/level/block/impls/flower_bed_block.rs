//! `net.minecraft.world.level.block.FlowerBlock`-like `FlowerBedBlock` (pink petals, wildflowers; extends
//! `VegetationBlock`, implements `SegmentableBlock`).
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;
use crate::server::level::ServerLevel;
use crate::world::item::{BlockPlaceContext, ItemStack};
use crate::world::level::block::bonemeal::BonemealSource;
use mc_data::props::{EnumProperty, IntegerProperty};

pub struct FlowerBedBlock;
impl MayPlaceOn for FlowerBedBlock {}

/// `SegmentableBlock.canBeReplaced(state, context, segment)`: another segment of the same block can be added.
pub fn segment_can_be_replaced(state: BlockState, context: &BlockPlaceContext, segment: IntegerProperty) -> bool {
    let held = context.use_on.get_item_in_hand();
    !context.is_secondary_use_active() && !held.is_empty() && Some(held.item()) == state.block().item() && state.get(segment) < 4
}

/// `SegmentableBlock.getStateForPlacement(context, block, segment, facing)`.
pub fn segment_state_for_placement(level: &dyn BlockGetter, context: &BlockPlaceContext, block: Block, segment: IntegerProperty, facing: EnumProperty<Direction>) -> BlockState {
    let state = level.get_block_state(context.get_clicked_pos());
    if state.is_block(block) {
        state.set(segment, 4.min(state.get(segment) + 1))
    } else {
        block.default_state().set(facing, context.get_horizontal_direction().get_opposite())
    }
}

impl BlockBehaviour for FlowerBedBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        super::vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        super::vegetation_block::update_shape(state, &*level, pos)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        if segment_can_be_replaced(state, context, props::FLOWER_AMOUNT) {
            return true;
        }
        let held = context.use_on.get_item_in_hand();
        state.can_be_replaced() && (held.is_empty() || Some(held.item()) != state.block().item())
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(segment_state_for_placement(&*level, context, block, props::FLOWER_AMOUNT, props::HORIZONTAL_FACING))
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        true
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        let current_amount = state.get(props::FLOWER_AMOUNT);
        if current_amount < 4 {
            level.set_block_flags(pos, state.set(props::FLOWER_AMOUNT, current_amount + 1), 2);
        } else if let Some(item) = state.block().item() {
            helpers::pop_resource(level, pos, ItemStack::new(item));
        }
    }
}

register_block_class!("FlowerBedBlock", |_| Box::new(FlowerBedBlock));
