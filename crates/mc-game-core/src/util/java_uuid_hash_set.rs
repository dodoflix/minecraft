//! `java.util.HashSet<UUID>` with Java's iteration order: bucket index `(h ^ (h >>> 16)) & (capacity - 1)` of
//! `UUID.hashCode()`, insertion order inside a bucket, table doubling at 0.75 load (a resize keeps the relative order of
//! the split buckets), `clear()` keeps the table, never shrinks.
//! shortcut: bins are never treeified (Java does at >= 8 colliding keys in a table of >= 64 buckets).

use mc_core::uuid_util::java_hash_code;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct UuidHashSet {
    table: Vec<Vec<Uuid>>,
    size: usize,
}

fn spread(uuid: &Uuid) -> usize {
    let h = java_hash_code(uuid);
    (h ^ ((h as u32) >> 16) as i32) as u32 as usize
}

impl UuidHashSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// `new HashSet<>(collection)`: the table is sized for `n` elements up front (`max(n / .75f + 1, 16)` rounded up to a
    /// power of two), then the elements are added in order.
    pub fn from_collection(items: &[Uuid]) -> Self {
        let wanted = ((items.len() as f32 / 0.75f32) as usize + 1).max(16);
        let mut set = Self { table: vec![Vec::new(); wanted.next_power_of_two()], size: 0 };
        for item in items {
            set.insert(*item);
        }
        set
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn contains(&self, uuid: &Uuid) -> bool {
        !self.table.is_empty() && self.table[spread(uuid) & (self.table.len() - 1)].contains(uuid)
    }

    /// `add(uuid)`: whether the set changed.
    pub fn insert(&mut self, uuid: Uuid) -> bool {
        if self.table.is_empty() {
            self.table = vec![Vec::new(); 16];
        }
        let index = spread(&uuid) & (self.table.len() - 1);
        if self.table[index].contains(&uuid) {
            return false;
        }
        self.table[index].push(uuid);
        self.size += 1;
        if self.size > self.table.len() * 3 / 4 {
            self.resize();
        }
        true
    }

    fn resize(&mut self) {
        let old_len = self.table.len();
        let mut table = vec![Vec::new(); old_len * 2];
        for (i, bucket) in std::mem::take(&mut self.table).into_iter().enumerate() {
            for uuid in bucket {
                let index = if spread(&uuid) & old_len == 0 { i } else { i + old_len };
                table[index].push(uuid);
            }
        }
        self.table = table;
    }

    pub fn remove(&mut self, uuid: &Uuid) -> bool {
        if self.table.is_empty() {
            return false;
        }
        let index = spread(uuid) & (self.table.len() - 1);
        match self.table[index].iter().position(|u| u == uuid) {
            Some(at) => {
                self.table[index].remove(at);
                self.size -= 1;
                true
            }
            None => false,
        }
    }

    /// `removeIf(predicate)` in iteration order; whether anything was removed.
    pub fn remove_if(&mut self, mut predicate: impl FnMut(&Uuid) -> bool) -> bool {
        let before = self.size;
        for bucket in &mut self.table {
            bucket.retain(|u| !predicate(u));
        }
        self.size = self.table.iter().map(Vec::len).sum();
        self.size != before
    }

    pub fn clear(&mut self) {
        for bucket in &mut self.table {
            bucket.clear();
        }
        self.size = 0;
    }

    /// Iteration order.
    pub fn iter(&self) -> impl Iterator<Item = &Uuid> {
        self.table.iter().flatten()
    }

    /// `addAll(other)`: whether the set changed.
    pub fn extend_from(&mut self, items: impl IntoIterator<Item = Uuid>) -> bool {
        let mut changed = false;
        for item in items {
            changed |= self.insert(item);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Order of `new HashSet<>(List.of(u1, u2, u3))` for UUIDs whose hashes land in distinct buckets of a 16-table.
    #[test]
    fn iteration_follows_bucket_index() {
        // hash = hi32 ^ lo32 of (msb ^ lsb); the low 4 bits pick the bucket
        let at = |bucket: u64| Uuid::from_u64_pair(0, bucket);
        let mut set = UuidHashSet::new();
        for b in [9, 3, 12] {
            set.insert(at(b));
        }
        let order: Vec<_> = set.iter().map(|u| u.as_u64_pair().1).collect();
        assert_eq!(order, vec![3, 9, 12]);
    }

    #[test]
    fn resize_keeps_every_element_and_relative_order() {
        let at = |n: u64| Uuid::from_u64_pair(0, n);
        let mut set = UuidHashSet::new();
        for n in 0..13 {
            set.insert(at(n * 16)); // all collide in bucket 0 of the 16-table
        }
        assert_eq!(set.len(), 13);
        // after the resize to 32, the odd multiples of 16 moved to bucket 16, even ones stayed
        let order: Vec<_> = set.iter().map(|u| u.as_u64_pair().1 / 16).collect();
        assert_eq!(order, vec![0, 2, 4, 6, 8, 10, 12, 1, 3, 5, 7, 9, 11]);
    }
}
