//! Shared plumbing of the village behaviors (POI predicates, `HashSet<BlockPos>` iteration order, same-dimension POI access).

use crate::server::level::ServerLevel;
use crate::world::entity::ai::brain::java_hash_map_order;
use crate::world::entity::ai::village::poi::PoiTypeHolder;
use mc_core::pos::BlockPos;
use mc_protocol::codec::GlobalPos;
use std::sync::Arc;

/// `Predicate<Holder<PoiType>>`.
pub type PoiPredicate = Arc<dyn Fn(PoiTypeHolder) -> bool + Send + Sync>;

/// `BiPredicate<ServerLevel, BlockPos>`.
pub type ValidPoi = Arc<dyn Fn(&ServerLevel, BlockPos) -> bool + Send + Sync>;

/// `holder -> holder.is(key)`.
pub fn poi_is(key: &'static str) -> PoiPredicate {
    Arc::new(move |t| t.is(key))
}

/// `BlockPos.hashCode()` (`Vec3i.hashCode`).
pub fn block_pos_hash(pos: BlockPos) -> i32 {
    pos.y().wrapping_add(pos.z().wrapping_mul(31)).wrapping_mul(31).wrapping_add(pos.x())
}

/// The iteration order of a `HashSet<BlockPos>` filled with `positions` (insertion order resolves bucket collisions).
pub fn java_block_pos_set_order(positions: &[BlockPos]) -> Vec<BlockPos> {
    let mut unique: Vec<BlockPos> = Vec::with_capacity(positions.len());
    for &p in positions {
        if !unique.contains(&p) {
            unique.push(p);
        }
    }
    let hashes: Vec<i32> = unique.iter().map(|&p| block_pos_hash(p)).collect();
    java_hash_map_order(&hashes).into_iter().map(|i| unique[i]).collect()
}

/// `level.getServer().getLevel(pos.dimension())` restricted to the level the behavior runs in: the POI manager of
/// `global`'s dimension, or `None` when that is another dimension (the server's other levels are not reachable from a level).
pub fn poi_level<'a>(level: &'a ServerLevel, global: &GlobalPos) -> Option<&'a ServerLevel> {
    (level.dimension == global.dimension).then_some(level)
}

/// `brain.updateActivityFromSchedule(level.environmentAttributes(), level.getGameTime(), body.position())`.
pub use super::update_activity_from_schedule::update as update_activity_from_schedule;
