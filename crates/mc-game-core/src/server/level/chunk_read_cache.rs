//! The four-entry chunk cache of `ServerChunkCache.getChunk` and what a block access through it owes the chunk map.
//!
//! `Level.setBlock` and `Level.getBlockState` on a `ServerLevel` are `getChunk(x, z, FULL, true)`: on a cache miss it
//! adds an `UNKNOWN` ticket (one tick) to the chunk and waits for it, so a chunk that was accessed this way lives a tick
//! longer than the tickets of players and commands say. The access is noted here and the ticket is added before the next
//! consumer of the tickets (the distance updates and the ticket purge). A plain read through `&self` that finds no chunk
//! cannot load it and notes it for the caller that can (`take_unloaded_read`).

use mc_core::pos::ChunkPos;
use std::cell::{Cell, RefCell};

/// `lastChunkPos` / `lastChunk` for the `FULL` status, plus the debts of the cache misses.
pub(crate) struct ChunkReadCache {
    last: [Cell<i64>; 4],
    owed: RefCell<Vec<i64>>,
    unloaded_read: Cell<Option<i64>>,
}

impl ChunkReadCache {
    pub(crate) fn new() -> Self {
        Self {
            last: std::array::from_fn(|_| Cell::new(ChunkPos::INVALID_CHUNK_POS)),
            owed: RefCell::new(Vec::new()),
            unloaded_read: Cell::new(None),
        }
    }

    /// `getChunk(x, z, FULL, true)` found the loaded chunk `key`: a cache miss owes the chunk an `UNKNOWN` ticket.
    pub(crate) fn read(&self, key: i64) {
        if self.in_cache(key) {
            return;
        }
        self.owed.borrow_mut().push(key);
        self.store(key);
    }

    /// `getChunk(x, z, FULL, false)` (`getChunkForCollisions`, `EntityFluidInteraction.hasFluidAndLoaded`) found the
    /// loaded chunk `key`: it is cached and owes no ticket, and a later read of the same tick finds it in the cache.
    pub(crate) fn cache_only(&self, key: i64) {
        if !self.in_cache(key) {
            self.store(key);
        }
    }

    fn in_cache(&self, key: i64) -> bool {
        self.last.iter().any(|slot| slot.get() == key)
    }

    /// `storeInCache`: the newest entry first.
    fn store(&self, key: i64) {
        for i in (1..4).rev() {
            self.last[i].set(self.last[i - 1].get());
        }
        self.last[0].set(key);
    }

    /// A read found no chunk at `key` (nothing is cached for an absent chunk).
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
            .for_each(|slot| slot.set(ChunkPos::INVALID_CHUNK_POS));
    }

    /// The chunks that missed the cache since the last call, oldest first.
    pub(crate) fn take_owed(&self) -> Vec<i64> {
        std::mem::take(&mut *self.owed.borrow_mut())
    }
}
