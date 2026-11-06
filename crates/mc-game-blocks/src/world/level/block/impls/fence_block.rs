//! `net.minecraft.world.level.block.FenceBlock` (extends `CrossCollisionBlock`).
use super::cross_collision_block;
use super::fence_gate_block;
use mc_core::phys::shapes::VoxelShape;
use mc_data::block::PathComputationType;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::collision::CollisionContext;
use mc_game_core::{register_block_class, update_shape};

pub struct FenceBlock;

/// `FenceBlock.connectsTo` (`this` is the fence block of `this_state`).
fn connects_to(
    this_state: BlockState,
    state: BlockState,
    face_solid: bool,
    direction: Direction,
) -> bool {
    let same_fence = state.is_tag("fences")
        && state.is_tag("wooden_fences")
            == this_state.block().default_state().is_tag("wooden_fences");
    let gate = helpers::is_instance_of(state, "FenceGateBlock")
        && fence_gate_block::connects_to_direction(state, direction);
    (!helpers::is_exception_for_connection(state) && face_solid) || same_fence || gate
}

impl BlockBehaviour for FenceBlock {
    mc_game_core::simple_waterlogged!();
    fn get_visual_shape(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        context: &CollisionContext,
    ) -> VoxelShape {
        self.get_shape(state, level, pos, context)
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_fluid_state = level.get_fluid_state(pos);
        let mut state = block.default_state();
        for (direction, opposite) in [
            (Direction::North, Direction::South),
            (Direction::East, Direction::West),
            (Direction::South, Direction::North),
            (Direction::West, Direction::East),
        ] {
            let neighbour_pos = pos.relative(direction);
            let neighbour = level.get_block_state(neighbour_pos);
            let connects = connects_to(
                state,
                neighbour,
                neighbour.is_face_sturdy_at(&*level, neighbour_pos, opposite),
                opposite,
            );
            state = state.set(helpers::pipe_property(direction), connects);
        }
        Some(state.set(
            props::WATERLOGGED,
            replaced_fluid_state.get_type() == mc_data::fluid::Fluid::WATER,
        ))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            if direction.get_axis().is_horizontal() {
                let opposite = direction.get_opposite();
                let sturdy = neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, opposite);
                state.set(
                    helpers::pipe_property(direction),
                    connects_to(state, neighbour_state, sturdy, opposite),
                )
            } else {
                state
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        cross_collision_block::rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        cross_collision_block::mirror(state, mirror)
    }
}

register_block_class!("FenceBlock", |_| Box::new(FenceBlock));
