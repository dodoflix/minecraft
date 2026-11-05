//! `net.minecraft.world.level.block.BeehiveBlock` (`bee_nest`, `beehive`).
use super::prelude::*;

pub struct BeehiveBlock;

impl BlockBehaviour for BeehiveBlock {
    // `updateShape` empties the hive's bees when fire is the neighbour: that acts on the `BeehiveBlockEntity` (not
    // ported yet); the returned state is unchanged.
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BeehiveBlock", |_| Box::new(BeehiveBlock));
