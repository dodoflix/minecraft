//! `net.minecraft.world.level.pathfinder.PathTypeCache`: the per-level cache of `WalkNodeEvaluator.getPathTypeFromState`
//! (invalidated by `ServerLevel.sendBlockUpdated`).

use super::path_type::PathType;
use super::walk_node_evaluator::get_path_type_from_state;
use crate::world::level::BlockGetter;
use mc_core::pos::BlockPos;

const SIZE: usize = 4096;
const MASK: i64 = 4095;

pub struct PathTypeCache {
    positions: Box<[i64; SIZE]>,
    path_types: Box<[Option<PathType>; SIZE]>,
}

impl Default for PathTypeCache {
    fn default() -> Self {
        PathTypeCache { positions: Box::new([0; SIZE]), path_types: Box::new([None; SIZE]) }
    }
}

/// fastutil `HashCommon.mix(long)` truncated to `int`.
fn mix(x: i64) -> i32 {
    let mut h = x.wrapping_mul(-7046029254386353131); // 0x9E3779B97F4A7C15
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32
}

fn index(pos: i64) -> usize {
    (i64::from(mix(pos)) & MASK) as usize
}

impl PathTypeCache {
    /// `getOrCompute(level, pos)`.
    pub fn get_or_compute<L: BlockGetter + ?Sized>(&mut self, level: &L, pos: BlockPos) -> PathType {
        let key = pos.as_long();
        let index = index(key);
        if self.positions[index] == key
            && let Some(cached) = self.path_types[index]
        {
            return cached;
        }
        let path_type = get_path_type_from_state(level, pos);
        self.positions[index] = key;
        self.path_types[index] = Some(path_type);
        path_type
    }

    /// `invalidate(pos)`.
    pub fn invalidate(&mut self, pos: BlockPos) {
        let key = pos.as_long();
        let index = index(key);
        if self.positions[index] == key {
            self.path_types[index] = None;
        }
    }

    /// The cached entry of `pos` (`get(index, key)`), for tests and the oracle dump.
    pub fn peek(&self, pos: BlockPos) -> Option<PathType> {
        let key = pos.as_long();
        let index = index(key);
        if self.positions[index] == key { self.path_types[index] } else { None }
    }
}
