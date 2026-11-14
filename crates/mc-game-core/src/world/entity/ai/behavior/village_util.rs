//! Shared plumbing of the village behaviors (POI predicates, `HashSet<BlockPos>` iteration order, same-dimension POI access).

use crate::server::level::ServerLevel;
use crate::util::java_hash_order::JavaHashCapacity;
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
    pos.y()
        .wrapping_add(pos.z().wrapping_mul(31))
        .wrapping_mul(31)
        .wrapping_add(pos.x())
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
    java_hash_map_order(&hashes)
        .into_iter()
        .map(|i| unique[i])
        .collect()
}

/// `Collectors.toSet()` of `Pair<Holder<PoiType>, BlockPos>`: the `HashSet` iteration order (`Pair.hashCode` is
/// `Objects.hashCode(first, second)` = `31 * (31 + first.hashCode()) + second.hashCode()`, the holder hashes like the oracle's).
pub fn java_poi_set_order(pois: Vec<(PoiTypeHolder, BlockPos)>) -> Vec<(PoiTypeHolder, BlockPos)> {
    let mut unique: Vec<(PoiTypeHolder, BlockPos)> = Vec::with_capacity(pois.len());
    for poi in pois {
        if !unique.contains(&poi) {
            unique.push(poi);
        }
    }
    let mut capacity = JavaHashCapacity::default();
    capacity.grow_to_fit(unique.len());
    capacity.iteration_order(unique, |(holder, pos)| {
        31i32
            .wrapping_add(holder.hash_code())
            .wrapping_mul(31)
            .wrapping_add(block_pos_hash(*pos))
    })
}

/// `level.getServer().getLevel(pos.dimension())` restricted to the level the behavior runs in: the POI manager of
/// `global`'s dimension, or `None` when that is another dimension (the server's other levels are not reachable from a level).
pub fn poi_level<'a>(level: &'a ServerLevel, global: &GlobalPos) -> Option<&'a ServerLevel> {
    (level.dimension == global.dimension).then_some(level)
}

/// `brain.updateActivityFromSchedule(level.environmentAttributes(), level.getGameTime(), body.position())`.
pub use super::update_activity_from_schedule::update as update_activity_from_schedule;

#[cfg(test)]
mod tests {
    use super::*;

    /// Two beds of a village: the buckets of their pair hashes (0 and 6) put the west bed first, as the oracle's
    /// `HashSet` iterates them.
    #[test]
    fn pair_set_iterates_by_bucket() {
        let home = PoiTypeHolder::by_name("minecraft:home").expect("home");
        let east = (home, BlockPos::new(3, -60, -1));
        let west = (home, BlockPos::new(-3, -60, -1));
        assert_eq!(java_poi_set_order(vec![east, west, east]), vec![west, east]);
    }
}
