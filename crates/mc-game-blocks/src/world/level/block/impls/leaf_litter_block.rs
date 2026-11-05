//! `net.minecraft.world.level.block.LeafLitterBlock` (extends `VegetationBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct LeafLitterBlock;

impl BlockBehaviour for LeafLitterBlock {
    fn can_be_replaced(&self, state: BlockState, context: &mc_game_core::world::item::BlockPlaceContext) -> bool {
        super::segmentable_block::can_be_replaced(state, context, props::SEGMENT_AMOUNT) || mc_game_core::world::level::block::PlainBlock.can_be_replaced(state, context)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut mc_game_core::server::level::ServerLevel, context: &mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
        Some(super::segmentable_block::get_state_for_placement(&*level, context, block, props::SEGMENT_AMOUNT, props::HORIZONTAL_FACING))
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        level.get_block_state(below_pos).is_face_sturdy_at(level, below_pos, Direction::Up)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        mc_game_core::world::level::block::impls::vegetation_block::update_shape(state, &*level, pos)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("LeafLitterBlock", |_| Box::new(LeafLitterBlock));
