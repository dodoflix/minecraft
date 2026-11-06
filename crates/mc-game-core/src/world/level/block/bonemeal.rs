//! `BonemealableBlock` statics and enums (the instance methods are `BlockBehaviour::{is_valid_bonemeal_target, ...}`).
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::{LevelAccessor, LevelReader};
use mc_core::direction::Plane;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;

/// `BonemealSource` (what applies the bone meal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonemealSource {
    Interaction,
    Mob,
}

/// `BonemealableBlock.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonemealType {
    NeighborSpreader,
    Grower,
}

fn spreadable_neighbour_pos(
    directions: &[mc_core::direction::Direction],
    level: &dyn LevelReader,
    pos: BlockPos,
    block_to_place: BlockState,
) -> Option<BlockPos> {
    directions
        .iter()
        .map(|d| pos.relative(*d))
        .find(|n| level.is_empty_block(*n) && block_to_place.can_survive(level, *n))
}

/// `BonemealableBlock.hasSpreadableNeighbourPos`.
pub fn has_spreadable_neighbour_pos(
    level: &dyn LevelReader,
    pos: BlockPos,
    block_to_place: BlockState,
) -> bool {
    spreadable_neighbour_pos(Plane::Horizontal.faces(), level, pos, block_to_place).is_some()
}

/// `BonemealableBlock.findSpreadableNeighbourPos` (shuffles the directions with the level random).
pub fn find_spreadable_neighbour_pos(
    level: &mut ServerLevel,
    pos: BlockPos,
    block_to_place: BlockState,
) -> Option<BlockPos> {
    let directions = Plane::Horizontal.shuffled_copy(level.random());
    spreadable_neighbour_pos(&directions, &*level, pos, block_to_place)
}
