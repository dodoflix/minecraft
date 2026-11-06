//! `net.minecraft.world.level.block.GrindstoneBlock` (extends `FaceAttachedHorizontalDirectionalBlock`).
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct GrindstoneBlock;

impl BlockBehaviour for GrindstoneBlock {
    fn can_survive(&self, _state: BlockState, _level: &dyn LevelReader, _pos: BlockPos) -> bool {
        true
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            // `FaceAttachedHorizontalDirectionalBlock.updateShape` (never removes: `canSurvive` is always true)
            state
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("GrindstoneBlock", |_| Box::new(GrindstoneBlock));
