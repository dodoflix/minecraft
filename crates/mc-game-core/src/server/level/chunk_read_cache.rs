//! The four-entry chunk cache of `ServerChunkCache.getChunk` and `getChunkNow`, and what a block access through it owes
//! the chunk map.
//!
//! `Level.setBlock` and `Level.getBlockState` on a `ServerLevel` are `getChunk(x, z, FULL, true)`: on a cache miss it
//! adds an `UNKNOWN` ticket (one tick) to the chunk and waits for it, so a chunk that was accessed this way lives a tick
//! longer than the tickets of players and commands say. The access is noted here and the ticket is added before the next
//! consumer of the tickets (the distance updates and the ticket purge). A plain read through `&self` that finds no chunk
//! cannot load it and notes it for the caller that can (`take_unloaded_read`).

use mc_core::pos::ChunkPos;
use std::cell::{Cell, RefCell};

/// `lastChunkPos` / `lastChunk` for the `FULL` status (an entry is the position and whether a chunk was found there:
/// vanilla caches the null of `getChunk(x, z, FULL, false)` too), plus the debts of the cache misses.
pub(crate) struct ChunkReadCache {
    last: [Cell<(i64, bool)>; 4],
    owed: RefCell<Vec<i64>>,
    unloaded_read: Cell<Option<i64>>,
}

impl ChunkReadCache {
    pub(crate) fn new() -> Self {
        Self {
            last: std::array::from_fn(|_| Cell::new((ChunkPos::INVALID_CHUNK_POS, false))),
            owed: RefCell::new(Vec::new()),
            unloaded_read: Cell::new(None),
        }
    }

    /// `getChunk(x, z, FULL, true)` found the loaded chunk `key`: a cache miss (an absent entry does not count as a hit)
    /// owes the chunk an `UNKNOWN` ticket.
    pub(crate) fn read(&self, key: i64) {
        if self.last.iter().any(|slot| slot.get() == (key, true)) {
            return;
        }
        self.owed.borrow_mut().push(key);
        self.store(key, true);
    }

    /// `getChunk(x, z, FULL, false)` (`getChunkForCollisions`, `EntityFluidInteraction.hasFluidAndLoaded`) and
    /// `getChunkNow`: the cached answer for `key` (newest entry first), if there is one.
    pub(crate) fn cached(&self, key: i64) -> Option<bool> {
        self.last
            .iter()
            .map(Cell::get)
            .find(|&(pos, _)| pos == key)
            .map(|(_, found)| found)
    }

    /// `storeInCache`: the newest entry first. Both calls that look a chunk up without loading it store what they found
    /// when [`Self::cached`] had no answer, a `getChunkNow` that finds no chunk stores nothing.
    pub(crate) fn store(&self, key: i64, found: bool) {
        for i in (1..4).rev() {
            self.last[i].set(self.last[i - 1].get());
        }
        self.last[0].set((key, found));
    }

    /// A plain read found no chunk at `key`: the caller that can load it does (a `getChunk(x, z, FULL, true)` that waits).
    pub(crate) fn note_unloaded_read(&self, key: i64) {
        if self.unloaded_read.get().is_none() {
            self.unloaded_read.set(Some(key));
        }
    }

    /// The first chunk a read missed since the last call, if any.
    pub(crate) fn take_unloaded_read(&self) -> Option<i64> {
        self.unloaded_read.take()
    }

    /// `clearCache()`.
    pub(crate) fn clear(&self) {
        self.last
            .iter()
            .for_each(|slot| slot.set((ChunkPos::INVALID_CHUNK_POS, false)));
    }

    /// The chunks that missed the cache since the last call, oldest first.
    pub(crate) fn take_owed(&self) -> Vec<i64> {
        std::mem::take(&mut *self.owed.borrow_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkReadCache;

    #[test]
    fn an_absent_entry_is_a_hit_for_lookups_but_not_for_reads() {
        let cache = ChunkReadCache::new();
        cache.store(7, false);
        assert_eq!(cache.cached(7), Some(false));
        cache.read(7);
        assert_eq!(cache.take_owed(), vec![7]);
        assert_eq!(cache.cached(7), Some(true));
    }

    #[test]
    fn four_entries_are_kept_and_the_oldest_is_evicted() {
        let cache = ChunkReadCache::new();
        for key in 1..=5 {
            cache.store(key, key % 2 == 0);
        }
        assert_eq!(cache.cached(1), None);
        assert_eq!(cache.cached(2), Some(true));
        assert_eq!(cache.cached(5), Some(false));
        cache.clear();
        assert_eq!(cache.cached(5), None);
    }
}
