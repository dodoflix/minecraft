//! `net.minecraft.world.level.block.TripWireBlock`.
use super::prelude::*;

pub struct TripWireBlock;

/// `TripWireBlock.shouldConnectTo` (`this` is the tripwire, its hook is `Blocks.TRIPWIRE_HOOK`).
fn should_connect_to(this: Block, block_state: BlockState, direction: Direction) -> bool {
    if block_state.is_block(Block::TRIPWIRE_HOOK) {
        block_state.get(props::HORIZONTAL_FACING) == direction.get_opposite()
    } else {
        block_state.is_block(this)
    }
}

impl BlockBehaviour for TripWireBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_axis().is_horizontal() {
            state.set(helpers::pipe_property(direction), should_connect_to(state.block(), neighbour_state, direction))
        } else {
            state
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(state, rotation, [props::NORTH, props::EAST, props::SOUTH, props::WEST])
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(state, mirror, [props::NORTH, props::EAST, props::SOUTH, props::WEST])
    }
}

register_block_class!("TripWireBlock", |_| Box::new(TripWireBlock));
