//! The surface of `Bee` that block entities use (`BeehiveBlockEntity.addOccupant`, `releaseOccupant`), exposed by the bee
//! class through `Entity::as_bee` / `Entity::as_bee_mut` because the class lives in a crate the block entities cannot
//! depend on.

use mc_core::pos::BlockPos;

pub trait BeeAccess {
    /// `getSavedFlowerPos()`.
    fn get_saved_flower_pos(&self) -> Option<BlockPos>;
    /// `setSavedFlowerPos(pos)`.
    fn set_saved_flower_pos(&mut self, pos: BlockPos);
    /// `setHivePos(pos)`.
    fn set_hive_pos(&mut self, pos: BlockPos);
    /// `dropOffNectar()`.
    fn drop_off_nectar(&mut self);
    /// `setStayOutOfHiveCountdown(ticks)`.
    fn set_stay_out_of_hive_countdown(&mut self, ticks: i32);
}
