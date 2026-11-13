//! The surface of `Bee` that block entities use (`BeehiveBlockEntity.addOccupant`), exposed by the bee class through
//! `Entity::as_bee` because the class lives in a crate the block entities cannot depend on.

use mc_core::pos::BlockPos;

pub trait BeeAccess {
    /// `getSavedFlowerPos()`.
    fn get_saved_flower_pos(&self) -> Option<BlockPos>;
}
