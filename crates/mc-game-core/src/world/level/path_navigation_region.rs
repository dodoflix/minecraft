//! `net.minecraft.world.level.PathNavigationRegion`: the chunks around a path query as seen by the path finder
//! (chunks that are not loaded, or outside the region, read as empty `VOID_AIR` chunks; no entity collisions).
//!
//! Vanilla copies the chunk references up front; the region here borrows the level for the duration of the query, which
//! is equivalent because nothing mutates the level while a path is computed.

use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::level::collision::{CollisionContext, CollisionGetter};
use crate::world::level::pathfinder::{PathLevel, PathTypeCache, PathfindingMob};
use crate::world::level::{BlockGetter, LevelHeight};
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;
use parking_lot::Mutex;

pub struct PathNavigationRegion<'a> {
    level: &'a ServerLevel,
    center_x: i32,
    center_z: i32,
    /// `chunks[x][z] != null`, flattened `x * depth + z`.
    loaded: Vec<bool>,
    width: i32,
    depth: i32,
}

impl<'a> PathNavigationRegion<'a> {
    /// `new PathNavigationRegion(level, start, end)`.
    pub fn new(level: &'a ServerLevel, start: BlockPos, end: BlockPos) -> Self {
        let center_x = start.x() >> 4;
        let center_z = start.z() >> 4;
        let xc2 = end.x() >> 4;
        let zc2 = end.z() >> 4;
        let width = (xc2 - center_x + 1).max(0);
        let depth = (zc2 - center_z + 1).max(0);
        let mut loaded = Vec::with_capacity((width * depth) as usize);
        for xc in center_x..=xc2 {
            for zc in center_z..=zc2 {
                loaded.push(level.chunk_now(xc, zc).is_some());
            }
        }
        PathNavigationRegion {
            level,
            center_x,
            center_z,
            loaded,
            width,
            depth,
        }
    }

    /// The level's path type cache (`serverLevel.getPathTypeCache()`).
    pub fn path_type_cache(&self) -> &'a Mutex<PathTypeCache> {
        self.level.path_type_cache()
    }

    pub fn level(&self) -> &'a ServerLevel {
        self.level
    }

    /// Whether the chunk at `(chunk_x, chunk_z)` is a real (loaded) chunk of this region.
    fn chunk_loaded(&self, chunk_x: i32, chunk_z: i32) -> bool {
        let (xc, zc) = (chunk_x - self.center_x, chunk_z - self.center_z);
        xc >= 0
            && xc < self.width
            && zc >= 0
            && zc < self.depth
            && self.loaded[(xc * self.depth + zc) as usize]
    }
}

impl BlockGetter for PathNavigationRegion<'_> {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        if self.is_outside_build_height(pos.y()) {
            return Block::AIR.default_state();
        }
        if self.chunk_loaded(pos.x() >> 4, pos.z() >> 4) {
            self.level.get_block_state(pos)
        } else {
            // `EmptyLevelChunk.getBlockState`
            Block::VOID_AIR.default_state()
        }
    }

    fn get_fluid_state(&self, pos: BlockPos) -> FluidState {
        if self.is_outside_build_height(pos.y()) || !self.chunk_loaded(pos.x() >> 4, pos.z() >> 4) {
            return FluidState::EMPTY;
        }
        self.level.get_fluid_state(pos)
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<&dyn BlockEntity> {
        if self.chunk_loaded(pos.x() >> 4, pos.z() >> 4) {
            self.level.get_block_entity(pos)
        } else {
            None
        }
    }

    fn level_height(&self) -> LevelHeight {
        self.level.level_height()
    }
}

impl CollisionGetter for PathNavigationRegion<'_> {
    // `getEntityCollisions` is `List.of()` (the default), `getChunkForCollisions` never null (the default).
}

impl PathLevel for PathNavigationRegion<'_> {
    fn no_collision(&self, mob: &dyn PathfindingMob, aabb: &AABB) -> bool {
        let context: CollisionContext = mob.collision_context();
        if !self.no_block_collision(Some(&context), aabb) {
            return false;
        }
        // `noBorderCollision(mob, aabb)` against the level's world border
        match self.level.world_border_ref() {
            Some(border) if border.is_inside_close_to_border(mob.x(), mob.z(), aabb) => {
                !mc_core::phys::shapes::Shapes::join_is_not_empty(
                    &border.get_collision_shape(),
                    &mc_core::phys::shapes::Shapes::create(*aabb),
                    mc_core::phys::shapes::BooleanOp::AND,
                )
            }
            _ => true,
        }
    }

    fn sea_level(&self) -> i32 {
        crate::world::level::LevelReader::get_sea_level(self.level)
    }
}

/// `getPathTypeStatic(mob, pos)` runs against the level itself.
impl PathLevel for ServerLevel {
    fn no_collision(&self, mob: &dyn PathfindingMob, aabb: &AABB) -> bool {
        let context = mob.collision_context();
        self.no_block_collision(Some(&context), aabb)
            && self.no_entity_collision(Some(mob.id()), aabb)
            && {
                match self.world_border_ref() {
                    Some(border) if border.is_inside_close_to_border(mob.x(), mob.z(), aabb) => {
                        !mc_core::phys::shapes::Shapes::join_is_not_empty(
                            &border.get_collision_shape(),
                            &mc_core::phys::shapes::Shapes::create(*aabb),
                            mc_core::phys::shapes::BooleanOp::AND,
                        )
                    }
                    _ => true,
                }
            }
    }

    fn sea_level(&self) -> i32 {
        crate::world::level::LevelReader::get_sea_level(self)
    }
}
