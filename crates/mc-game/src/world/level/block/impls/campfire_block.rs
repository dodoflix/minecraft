//! `net.minecraft.world.level.block.CampfireBlock`.
use super::prelude::*;
use crate::world::level::collision::CollisionContext;
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use std::sync::LazyLock;

/// `SHAPE_VIRTUAL_POST`.
static SHAPE_VIRTUAL_POST: LazyLock<VoxelShape> = LazyLock::new(|| crate::world::level::shapes::column(4.0, 0.0, 16.0));

pub struct CampfireBlock;

impl CampfireBlock {
    /// `isLitCampfire(state)`.
    pub fn is_lit_campfire(state: BlockState) -> bool {
        state.has_property(props::LIT) && state.is_tag("campfires") && state.get(props::LIT)
    }

    /// `isSmokeyPos(level, pos)`: a lit campfire up to 5 blocks below with no smoke-blocking collision in between.
    pub fn is_smokey_pos(level: &dyn BlockGetter, pos: BlockPos) -> bool {
        for i in 1..=5 {
            let pos_to_check = pos.below_n(i);
            let state = level.get_block_state(pos_to_check);
            if Self::is_lit_campfire(state) {
                return true;
            }
            use crate::world::level::block::BlockStateExt as _;
            let blocked = Shapes::join_is_not_empty(&SHAPE_VIRTUAL_POST, &state.get_collision_shape_ctx(level, pos, &CollisionContext::empty()), BooleanOp::AND);
            if blocked {
                return Self::is_lit_campfire(level.get_block_state(pos_to_check.below()));
            }
        }
        false
    }
}

impl BlockBehaviour for CampfireBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction == Direction::Down { state.set(props::SIGNAL_FIRE, neighbour_state.is_block(Block::HAY_BLOCK)) } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CampfireBlock", |_| Box::new(CampfireBlock));
