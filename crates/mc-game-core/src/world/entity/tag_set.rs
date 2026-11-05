//! `HashSet<String>` with Java's iteration order (entity `Tags` are saved in `HashSet` order).
use mc_core::math::java::java_string_hash;

/// A set of strings iterating like a `java.util.HashSet<String>` filled in insertion order: bucket index of
/// `hash ^ (hash >>> 16)` in a power-of-two table that doubled at each 0.75 load factor crossing and never
/// shrinks; entries of one bucket keep insertion order (resizing preserves relative order). Trees (>= 8 collisions
/// in a bucket) are not modelled.
#[derive(Clone, Debug, Default)]
pub struct JavaStringSet {
    items: Vec<String>,
    capacity: usize,
}

fn spread(s: &str) -> u32 {
    let h = java_string_hash(s) as u32;
    h ^ (h >> 16)
}

impl JavaStringSet {
    pub fn new() -> Self {
        Self { items: Vec::new(), capacity: 16 }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, s: &str) -> bool {
        self.items.iter().any(|i| i == s)
    }

    pub fn insert(&mut self, s: impl Into<String>) -> bool {
        let s = s.into();
        if self.contains(&s) {
            return false;
        }
        self.items.push(s);
        if self.capacity == 0 {
            self.capacity = 16;
        }
        while self.items.len() > self.capacity * 3 / 4 {
            self.capacity *= 2;
        }
        true
    }

    pub fn remove(&mut self, s: &str) -> bool {
        match self.items.iter().position(|i| i == s) {
            Some(p) => {
                self.items.remove(p);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Iteration order of the Java set.
    pub fn iter_java_order(&self) -> Vec<&str> {
        let cap = self.capacity.max(16) as u32;
        let mut v: Vec<(u32, &str)> = self.items.iter().map(|s| (spread(s) & (cap - 1), s.as_str())).collect();
        v.sort_by_key(|(bucket, _)| *bucket); // stable: insertion order inside a bucket
        v.into_iter().map(|(_, s)| s).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_set_orders_by_bucket() {
        // Java: new HashSet<>(List.of("b","a","c")) iterates a, b, c ("a"=97 -> bucket 1, "b" -> 2, "c" -> 3)
        let mut s = JavaStringSet::new();
        s.insert("c");
        s.insert("b");
        s.insert("a");
        assert_eq!(s.iter_java_order(), ["a", "b", "c"]);
    }
}
