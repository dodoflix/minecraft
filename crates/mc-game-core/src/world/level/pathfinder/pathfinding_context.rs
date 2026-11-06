//! `net.minecraft.world.level.pathfinder.PathfindingContext`.

use super::mob_view::PathfindingMob;
use super::path_type::PathType;
use super::path_type_cache::PathTypeCache;
use super::walk_node_evaluator::get_path_type_from_state;
use crate::world::level::BlockGetter;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use parking_lot::Mutex;

/// The `CollisionGetter` a context reads: a [`PathNavigationRegion`](crate::world::level::path_navigation_region::PathNavigationRegion)
/// (or the level itself for the one-off `getPathTypeStatic(mob, pos)`).
pub trait PathLevel: BlockGetter {
    /// `noCollision(mob, aabb)`.
    fn no_collision(&self, mob: &dyn PathfindingMob, aabb: &AABB) -> bool;
    /// `getSeaLevel()` of the level the mob is in.
    fn sea_level(&self) -> i32;
}

pub struct PathfindingContext<'a> {
    level: &'a dyn PathLevel,
    cache: Option<&'a Mutex<PathTypeCache>>,
    mob_position: BlockPos,
}

impl<'a> PathfindingContext<'a> {
    /// `new PathfindingContext(level, mob)`; `cache` is `serverLevel.getPathTypeCache()`.
    pub fn new(
        level: &'a dyn PathLevel,
        cache: Option<&'a Mutex<PathTypeCache>>,
        mob: &dyn PathfindingMob,
    ) -> Self {
        PathfindingContext {
            level,
            cache,
            mob_position: mob.block_position(),
        }
    }

    /// `getPathTypeFromState(x, y, z)`.
    pub fn get_path_type_from_state(&self, x: i32, y: i32, z: i32) -> PathType {
        let pos = BlockPos::new(x, y, z);
        match self.cache {
            None => get_path_type_from_state(self.level, pos),
            Some(cache) => {
                // Cached (and filled) under the lock: `get_path_type_from_state` never touches the cache.
                cache.lock().get_or_compute(self.level, pos)
            }
        }
    }

    /// `getBlockState(pos)`.
    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        self.level.get_block_state(pos)
    }

    pub fn level(&self) -> &'a dyn PathLevel {
        self.level
    }

    /// `mobPosition()`.
    pub fn mob_position(&self) -> BlockPos {
        self.mob_position
    }
}
