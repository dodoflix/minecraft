//! `net.minecraft.world.level.block.CrossCollisionBlock` (shared by `FenceBlock` and `IronBarsBlock`).
use mc_game_core::world::level::block::impls::prelude::*;

/// `CrossCollisionBlock.rotate`.
pub fn rotate(state: BlockState, rotation: Rotation) -> BlockState {
    helpers::rotate_sides(
        state,
        rotation,
        [props::NORTH, props::EAST, props::SOUTH, props::WEST],
    )
}

/// `CrossCollisionBlock.mirror`.
pub fn mirror(state: BlockState, mirror: Mirror) -> BlockState {
    helpers::mirror_sides(
        state,
        mirror,
        [props::NORTH, props::EAST, props::SOUTH, props::WEST],
    )
}
