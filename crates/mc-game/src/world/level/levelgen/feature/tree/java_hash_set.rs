//! `java.util.HashSet<BlockPos>` with Java's iteration order. The tree pipeline copies its `Sets.newHashSet()` position
//! sets into lists (`TreeDecorator.Context`, `FallenTreeFeature`) that are sorted *stably* by Y, so the order of equal-Y
//! positions is the set's iteration order: bucket index ascending, insertion order within a bucket, buckets split in
//! order when the table doubles at 0.75 load (a bucket reaching 9 nodes in a table below 64 resizes too).
//! shortcut: bins are never treeified (Java does at >= 9 colliding nodes in a table of >= 64 buckets; the resulting order
//! for equal-hash keys depends on `System.identityHashCode`, so vanilla itself is not deterministic there).

use mc_core::pos::BlockPos;

#[derive(Default, Clone)]
pub struct BlockPosHashSet {
    table: Vec<Vec<BlockPos>>,
    size: usize,
    threshold: usize,
}

/// `HashMap.hash(key)` of `Vec3i.hashCode()`.
fn hash(pos: &BlockPos) -> u32 {
    let h = pos.y().wrapping_add(pos.z().wrapping_mul(31)).wrapping_mul(31).wrapping_add(pos.x()) as u32;
    h ^ (h >> 16)
}

impl BlockPosHashSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn contains(&self, pos: &BlockPos) -> bool {
        !self.table.is_empty() && self.table[hash(pos) as usize & (self.table.len() - 1)].contains(pos)
    }

    /// `add(pos)`: whether the set changed.
    pub fn insert(&mut self, pos: BlockPos) -> bool {
        if self.table.is_empty() {
            self.resize();
        }
        let index = hash(&pos) as usize & (self.table.len() - 1);
        let bin = &mut self.table[index];
        if bin.contains(&pos) {
            return false;
        }
        bin.push(pos);
        if bin.len() >= 9 && self.table.len() < 64 {
            self.resize();
        }
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        true
    }

    fn resize(&mut self) {
        let old_cap = self.table.len();
        let new_cap = if old_cap == 0 { 16 } else { old_cap * 2 };
        let mut table: Vec<Vec<BlockPos>> = vec![Vec::new(); new_cap];
        for (j, bin) in std::mem::take(&mut self.table).into_iter().enumerate() {
            for pos in bin {
                let index = if hash(&pos) as usize & old_cap == 0 { j } else { j + old_cap };
                table[index].push(pos);
            }
        }
        self.table = table;
        self.threshold = new_cap / 4 * 3;
    }

    /// Iteration order of the Java set.
    pub fn iter(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.table.iter().flatten().copied()
    }
}
