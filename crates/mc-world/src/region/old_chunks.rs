//! `IOWorker.isOldChunkAround`: a bit per chunk of a region file telling whether the stored chunk predates
//! `DataVersion` 4997 or carries `blending_data`, kept for the 1 024 most recently used regions.

use indexmap::IndexMap;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, Tag};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// `IOWorker.REGION_CACHE_SIZE`.
const REGION_CACHE_SIZE: usize = 1024;
/// The first `DataVersion` whose chunks are blended with new terrain only through stored `blending_data`.
const BLENDING_DATA_VERSION: i32 = 4997;

/// The `BitSet` of one region file, bit `z * 32 + x` per chunk.
#[derive(Clone, Default)]
pub struct OldChunkBits([u64; 16]);

impl OldChunkBits {
    pub fn set(&mut self, local_x: i32, local_z: i32) {
        let index = (local_z * 32 + local_x) as usize;
        self.0[index >> 6] |= 1 << (index & 63);
    }

    fn get(&self, local_x: i32, local_z: i32) -> bool {
        let index = (local_z * 32 + local_x) as usize;
        self.0[index >> 6] & (1 << (index & 63)) != 0
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(|&word| word == 0)
    }
}

/// `IOWorker.isOldChunk(tag)` over the fields `isOldChunkAround` collects: the `DataVersion` counts only as an `IntTag`
/// (anything else reads as 0), `blending_data` only as a compound.
pub fn is_old_chunk(tag: &CompoundTag) -> bool {
    let version = match tag.get("DataVersion") {
        Some(Tag::Int(version)) => *version,
        _ => 0,
    };
    version < BLENDING_DATA_VERSION || matches!(tag.get("blending_data"), Some(Tag::Compound(_)))
}

/// `IOWorker.regionCacheForBlender`: most recently used first (`getAndMoveToFirst`, `putAndMoveToFirst`, `removeLast`), a
/// region's bits computed once (the `CompletableFuture` of vanilla: threads that ask for the region meanwhile wait for the
/// same scan). Clones share the cache; it is locked only to find or insert a region, never while one is scanned.
#[derive(Clone, Default)]
pub struct OldChunkRegions(Arc<Mutex<IndexMap<i64, Arc<OnceLock<OldChunkBits>>>>>);

impl OldChunkRegions {
    /// The cell of region (`region_x`, `region_z`), moved (or put) to the front.
    fn cell(&self, region_x: i32, region_z: i32) -> Arc<OnceLock<OldChunkBits>> {
        let key = ChunkPos::pack_xz(region_x, region_z);
        let mut regions = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = regions.get_index_of(&key) {
            regions.move_index(index, 0);
        } else {
            regions.insert(key, Arc::default());
            let last = regions.len() - 1;
            regions.move_index(last, 0);
            if regions.len() > REGION_CACHE_SIZE {
                regions.pop();
            }
        }
        regions[0].clone()
    }

    /// `isOldChunkAround(pos, range)`: whether any chunk in the square of `range` around `pos` is old; `scan_region`
    /// computes the bits of a region that is not cached yet.
    pub fn is_old_chunk_around(
        &self,
        pos: ChunkPos,
        range: i32,
        mut scan_region: impl FnMut(i32, i32) -> OldChunkBits,
    ) -> bool {
        let from = ChunkPos::new(pos.x - range, pos.z - range);
        let to = ChunkPos::new(pos.x + range, pos.z + range);
        for region_x in from.get_region_x()..=to.get_region_x() {
            for region_z in from.get_region_z()..=to.get_region_z() {
                let cell = self.cell(region_x, region_z);
                let bits = cell.get_or_init(|| scan_region(region_x, region_z));
                if bits.is_empty() {
                    continue;
                }
                let min = ChunkPos::min_from_region(region_x, region_z);
                let (start_x, start_z) = ((from.x - min.x).max(0), (from.z - min.z).max(0));
                let (end_x, end_z) = ((to.x - min.x).min(31), (to.z - min.z).min(31));
                for x in start_x..=end_x {
                    for z in start_z..=end_z {
                        if bits.get(x, z) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_chunk(local_x: i32, local_z: i32) -> OldChunkBits {
        let mut bits = OldChunkBits::default();
        bits.set(local_x, local_z);
        bits
    }

    #[test]
    fn a_region_is_scanned_once_and_the_least_recently_used_one_is_dropped() {
        let regions = OldChunkRegions::default();
        let scans = std::cell::Cell::new(0);
        let ask = |regions: &OldChunkRegions, region_x: i32| {
            regions.is_old_chunk_around(ChunkPos::new(region_x * 32, 0), 0, |_, _| {
                scans.set(scans.get() + 1);
                with_chunk(0, 0)
            })
        };
        assert!(ask(&regions, 0));
        assert!(ask(&regions, 0));
        for region_x in 1..=1023 {
            ask(&regions, region_x);
        }
        assert!(ask(&regions, 0), "still among the 1 024 newest");
        assert!(ask(&regions, 1024));
        assert_eq!(scans.get(), 1025);
        assert!(ask(&regions, 1));
        assert_eq!(
            scans.get(),
            1026,
            "region 1 was the oldest when region 1024 came"
        );
        assert!(ask(&regions, 0));
        assert_eq!(scans.get(), 1026);
    }

    #[test]
    fn only_chunks_inside_the_square_count() {
        let regions = OldChunkRegions::default();
        let near = |x, z, range| {
            regions.is_old_chunk_around(ChunkPos::new(x, z), range, |_, _| with_chunk(5, 7))
        };
        assert!(near(5, 7, 0));
        assert!(!near(6, 7, 0));
        assert!(near(8, 9, 3));
        assert!(!near(9, 9, 3));
        assert!(!near(8, 11, 3));
    }
}
