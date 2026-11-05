//! `net.minecraft.world.level.block.IronBarsBlock` (extends `CrossCollisionBlock`).
use super::prelude::*;
use super::cross_collision_block;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::collision::CollisionContext;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_data::block::PathComputationType;

pub struct IronBarsBlock;

/// `IronBarsBlock.attachsTo`.
fn attachs_to(state: BlockState, face_solid: bool) -> bool {
    (!helpers::is_exception_for_connection(state) && face_solid) || helpers::is_instance_of(state, "IronBarsBlock") || state.is_tag("walls")
}

impl BlockBehaviour for IronBarsBlock {
    crate::simple_waterlogged!();
    fn get_visual_shape(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, _context: &CollisionContext) -> VoxelShape {
        Shapes::empty()
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_fluid_state = level.get_fluid_state(pos);
        let mut state = block.default_state();
        for (direction, opposite) in [(Direction::North, Direction::South), (Direction::South, Direction::North), (Direction::West, Direction::East), (Direction::East, Direction::West)] {
            let neighbour_pos = pos.relative(direction);
            let neighbour = level.get_block_state(neighbour_pos);
            state = state.set(helpers::pipe_property(direction), attachs_to(neighbour, neighbour.is_face_sturdy_at(&*level, neighbour_pos, opposite)));
        }
        Some(state.set(props::WATERLOGGED, replaced_fluid_state.get_type() == mc_data::fluid::Fluid::WATER))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction.get_axis().is_horizontal() {
            let sturdy = neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, direction.get_opposite());
            state.set(helpers::pipe_property(direction), attachs_to(neighbour_state, sturdy))
        } else {
            state
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        cross_collision_block::rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        cross_collision_block::mirror(state, mirror)
    }
}

register_block_class!("IronBarsBlock", |_| Box::new(IronBarsBlock));
