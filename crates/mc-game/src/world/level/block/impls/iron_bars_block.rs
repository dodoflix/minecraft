//! `net.minecraft.world.level.block.IronBarsBlock` (extends `CrossCollisionBlock`).
use super::prelude::*;
use super::cross_collision_block;

pub struct IronBarsBlock;

/// `IronBarsBlock.attachsTo`.
fn attachs_to(state: BlockState, face_solid: bool) -> bool {
    (!helpers::is_exception_for_connection(state) && face_solid) || helpers::is_instance_of(state, "IronBarsBlock") || state.is_tag("walls")
}

impl BlockBehaviour for IronBarsBlock {
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
