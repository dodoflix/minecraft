//! TEMPORARY seam for MobsScope.Raids' `ServerLevel::get_raid_at` / `get_raid` (no raid exists until their port lands):
//! delete this file and use `crate::world::entity::raid` when it is on main.

use crate::server::level::ServerLevel;
use mc_core::pos::BlockPos;

/// The read-only `Raid` surface the village behaviors use.
pub struct RaidSeam;

impl RaidSeam {
    pub fn id(&self) -> i32 {
        0
    }
    pub fn is_active(&self) -> bool {
        false
    }
    pub fn is_over(&self) -> bool {
        false
    }
    pub fn is_victory(&self) -> bool {
        false
    }
    pub fn is_loss(&self) -> bool {
        false
    }
    pub fn is_stopped(&self) -> bool {
        false
    }
    pub fn is_between_waves(&self) -> bool {
        false
    }
    pub fn has_first_wave_spawned(&self) -> bool {
        false
    }
}

impl ServerLevel {
    /// `getRaidAt(pos)`.
    pub fn get_raid_at(&self, _pos: BlockPos) -> Option<RaidSeam> {
        None
    }
    /// `getRaids().get(id)`.
    pub fn get_raid(&self, _id: i32) -> Option<RaidSeam> {
        None
    }
}
