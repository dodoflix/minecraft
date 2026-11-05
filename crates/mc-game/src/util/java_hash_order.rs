//! Iteration order of a `java.util.HashMap`/`HashSet` (bucket index, then insertion order within the bucket), for
//! vanilla code whose observable output iterates one.

/// `HashMap.table.length` tracker: starts at 16, doubles when `size > 0.75 * capacity`, never shrinks.
#[derive(Clone, Copy, Debug)]
pub struct JavaHashCapacity(usize);

impl Default for JavaHashCapacity {
    fn default() -> Self {
        Self(16)
    }
}

impl JavaHashCapacity {
    /// Call after an insertion with the map's new `size`.
    pub fn grow_to_fit(&mut self, size: usize) {
        while size > self.0 / 4 * 3 {
            self.0 *= 2;
        }
    }

    /// `items` (in insertion order) in the iteration order of a hash table of this capacity (`hash` = `hashCode()`).
    pub fn iteration_order<T>(&self, items: impl IntoIterator<Item = T>, hash: impl Fn(&T) -> i32) -> Vec<T> {
        let bucket = |item: &T| {
            let h = hash(item) as u32;
            ((h ^ (h >> 16)) as usize) & (self.0 - 1)
        };
        let mut keyed: Vec<(usize, T)> = items.into_iter().map(|item| (bucket(&item), item)).collect();
        keyed.sort_by_key(|(bucket, _)| *bucket); // stable
        keyed.into_iter().map(|(_, item)| item).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_bucket_then_insertion() {
        let mut c = JavaHashCapacity::default();
        c.grow_to_fit(3);
        assert_eq!(c.iteration_order([17, 1, 33, 2, 16], |v| *v), vec![16, 17, 1, 33, 2]);
        c.grow_to_fit(13);
        assert_eq!(c.iteration_order([17, 1, 33, 2, 16], |v| *v), vec![1, 33, 2, 16, 17]);
    }
}
