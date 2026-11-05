//! `net.minecraft.world.level.block.FenceBlock` (extends `CrossCollisionBlock`).
use super::prelude::*;
use super::cross_collision_block;
use super::fence_gate_block;

pub struct FenceBlock;

/// `FenceBlock.connectsTo` (`this` is the fence block of `this_state`).
fn connects_to(this_state: BlockState, state: BlockState, face_solid: bool, direction: Direction) -> bool {
    let same_fence = state.is_tag("fences") && state.is_tag("wooden_fences") == this_state.block().default_state().is_tag("wooden_fences");
    let gate = helpers::is_instance_of(state, "FenceGateBlock") && fence_gate_block::connects_to_direction(state, direction);
    (!helpers::is_exception_for_connection(state) && face_solid) || same_fence || gate
}

impl BlockBehaviour for FenceBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction.get_axis().is_horizontal() {
            let opposite = direction.get_opposite();
            let sturdy = neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, opposite);
            state.set(helpers::pipe_property(direction), connects_to(state, neighbour_state, sturdy, opposite))
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

register_block_class!("FenceBlock", |_| Box::new(FenceBlock));
