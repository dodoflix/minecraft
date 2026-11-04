//! Java collection behaviour that is observable through randomness: `Util.shuffle` and `java.util.HashSet<BlockPos>` with Java's iteration order (bucket index, then insertion order inside a bucket).
//! Vanilla features that iterate such a set and consume randomness per element (`VegetationPatchFeature`) are only
//! reproducible with the same order.
//!
//! shortcut: bins are never treeified (Java does that at 8 colliding entries in a table of at least 64 buckets, which
//! reorders that bin); a patch would need 8 hash collisions in one bucket, upgrade if a golden test ever shows it.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

/// `Util.shuffle(list, random)`.
pub fn shuffle<T>(list: &mut [T], random: &mut dyn RandomSource) {
    let mut i = list.len();
    while i > 1 {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        list.swap(i - 1, swap_to);
        i -= 1;
    }
}

pub struct JavaHashSet {
    /// `HashMap.table`: empty until the first insertion, then a power of two buckets.
    table: Vec<Vec<BlockPos>>,
    size: usize,
}

impl Default for JavaHashSet {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaHashSet {
    pub fn new() -> Self {
        Self { table: Vec::new(), size: 0 }
    }

    /// `HashMap.hash(key)` spread folded into a bucket index.
    fn index(pos: &BlockPos, capacity: usize) -> usize {
        let h = pos.hash_code() as u32;
        ((h ^ (h >> 16)) as usize) & (capacity - 1)
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn contains(&self, pos: &BlockPos) -> bool {
        !self.table.is_empty() && self.table[Self::index(pos, self.table.len())].contains(pos)
    }

    /// `add(pos)`: returns whether the set changed.
    pub fn add(&mut self, pos: BlockPos) -> bool {
        if self.table.is_empty() {
            self.table = vec![Vec::new(); 16];
        }
        let idx = Self::index(&pos, self.table.len());
        if self.table[idx].contains(&pos) {
            return false;
        }
        self.table[idx].push(pos);
        self.size += 1;
        if self.size > self.table.len() / 4 * 3 {
            self.resize();
        }
        true
    }

    /// `resize()`: doubles the table; entries of one old bucket keep their relative order in the new buckets.
    fn resize(&mut self) {
        let new_capacity = self.table.len() * 2;
        let mut new_table: Vec<Vec<BlockPos>> = vec![Vec::new(); new_capacity];
        for bucket in std::mem::take(&mut self.table) {
            for pos in bucket {
                new_table[Self::index(&pos, new_capacity)].push(pos);
            }
        }
        self.table = new_table;
    }

    /// Iteration order of `HashSet.iterator()`.
    pub fn iter(&self) -> impl Iterator<Item = &BlockPos> {
        self.table.iter().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterates_in_bucket_order_after_resizes() {
        // hash(x, y, z) = (y + z * 31) * 31 + x; with a small hash the bucket is the hash itself modulo the capacity.
        let mut set = JavaHashSet::new();
        for x in [5, 3, 20, 1, 16] {
            set.add(BlockPos::new(x, 0, 0));
        }
        assert!(!set.add(BlockPos::new(3, 0, 0)));
        // capacity 16: bucket(20) = 4, bucket(16) = 0
        let xs: Vec<i32> = set.iter().map(BlockPos::x).collect();
        assert_eq!(xs, vec![16, 1, 3, 20, 5]);
        for x in 100..120 {
            set.add(BlockPos::new(x, 0, 0));
        }
        // 25 entries: 13th grows to 32 buckets, 25th to 64 buckets, bucket(x) = x mod 64 for these small hashes
        assert_eq!(set.len(), 25);
        let first: Vec<i32> = set.iter().take(5).map(BlockPos::x).collect();
        assert_eq!(first, vec![1, 3, 5, 16, 20]);
    }
}
