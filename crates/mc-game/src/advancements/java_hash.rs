//! `java.util.HashMap` / `HashSet` with Java's iteration order for keys whose `hashCode` is known (`Identifier`,
//! `String`, records of those). Vanilla's advancement packets and trigger evaluation iterate such collections, so the
//! order is observable (packet bytes, which advancement a trigger awards first).
//! shortcut: bins are never treeified (Java does at >= 8 colliding keys in a table of >= 64 buckets).

use mc_core::ident::Identifier;
use mc_core::math::java::java_string_hash;

/// A key with a Java `hashCode()`.
pub trait JavaHash: Eq {
    fn java_hash(&self) -> i32;
}

impl JavaHash for String {
    fn java_hash(&self) -> i32 {
        java_string_hash(self)
    }
}

impl JavaHash for Identifier {
    /// `31 * namespace.hashCode() + path.hashCode()`.
    fn java_hash(&self) -> i32 {
        java_string_hash(self.namespace()).wrapping_mul(31).wrapping_add(java_string_hash(self.path()))
    }
}

struct Node<K, V> {
    hash: i32,
    key: K,
    value: V,
}

pub struct JavaHashMap<K, V> {
    table: Vec<Vec<Node<K, V>>>,
    size: usize,
    threshold: usize,
    /// The first table size (`tableSizeFor(initialCapacity)`); 0 = the default 16.
    initial: usize,
}

impl<K, V> Default for JavaHashMap<K, V> {
    fn default() -> Self {
        JavaHashMap { table: Vec::new(), size: 0, threshold: 0, initial: 0 }
    }
}

/// `HashMap.tableSizeFor(cap)`.
fn table_size_for(cap: usize) -> usize {
    cap.max(1).next_power_of_two()
}

impl<K: JavaHash, V> JavaHashMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    fn spread(key: &K) -> i32 {
        let h = key.java_hash();
        h ^ ((h as u32) >> 16) as i32
    }

    fn index(&self, hash: i32) -> usize {
        (hash as u32 as usize) & (self.table.len() - 1)
    }

    /// `new HashMap<>(initialCapacity)`.
    pub fn with_initial_capacity(capacity: usize) -> Self {
        JavaHashMap { initial: table_size_for(capacity), ..Self::default() }
    }

    /// `new HashMap<>(source)` for a source of `size` entries given in its iteration order (the copy constructor
    /// presizes the table from the source's size).
    pub fn copy_of(entries: impl IntoIterator<Item = (K, V)>, size: usize) -> Self {
        let mut m = if size == 0 { Self::new() } else { Self::with_initial_capacity(((size as f32 / 0.75f32) + 1.0f32) as usize) };
        for (k, v) in entries {
            m.insert(k, v);
        }
        m
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        if self.table.is_empty() {
            return None;
        }
        let hash = Self::spread(key);
        self.table[self.index(hash)].iter().find(|n| n.hash == hash && n.key == *key).map(|n| &n.value)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        if self.table.is_empty() {
            return None;
        }
        let hash = Self::spread(key);
        let idx = self.index(hash);
        self.table[idx].iter_mut().find(|n| n.hash == hash && n.key == *key).map(|n| &mut n.value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// `put`: replaces in place (iteration position unchanged) or appends to the bin.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.table.is_empty() {
            self.resize();
        }
        let hash = Self::spread(&key);
        let idx = self.index(hash);
        if let Some(node) = self.table[idx].iter_mut().find(|n| n.hash == hash && n.key == key) {
            return Some(std::mem::replace(&mut node.value, value));
        }
        self.table[idx].push(Node { hash, key, value });
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.table.is_empty() {
            return None;
        }
        let hash = Self::spread(key);
        let idx = self.index(hash);
        let pos = self.table[idx].iter().position(|n| n.hash == hash && n.key == *key)?;
        self.size -= 1;
        Some(self.table[idx].remove(pos).value)
    }

    /// `clear`: empties the bins but keeps the table size.
    pub fn clear(&mut self) {
        for bin in &mut self.table {
            bin.clear();
        }
        self.size = 0;
    }

    fn resize(&mut self) {
        let new_cap = if !self.table.is_empty() {
            self.table.len() * 2
        } else if self.initial > 0 {
            self.initial
        } else {
            16
        };
        self.threshold = (new_cap as f32 * 0.75f32) as usize;
        let old = std::mem::take(&mut self.table);
        self.table = (0..new_cap).map(|_| Vec::new()).collect();
        // splitting a bin into lo/hi keeps the relative order, which equals re-appending in iteration order
        for node in old.into_iter().flatten() {
            let idx = (node.hash as u32 as usize) & (new_cap - 1);
            self.table[idx].push(node);
        }
    }

    /// Entries in Java iteration order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.table.iter().flatten().map(|n| (&n.key, &n.value))
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }
}

/// `HashSet<K>`.
pub struct JavaHashSet<K>(JavaHashMap<K, ()>);

impl<K> Default for JavaHashSet<K> {
    fn default() -> Self {
        JavaHashSet(JavaHashMap::default())
    }
}

impl<K: JavaHash> JavaHashSet<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `add`: whether the key was absent.
    pub fn insert(&mut self, key: K) -> bool {
        self.0.insert(key, ()).is_none()
    }

    pub fn remove(&mut self, key: &K) -> bool {
        self.0.remove(key).is_some()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Java iteration order.
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.0.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_keys_follow_java_bin_order() {
        // "a".hashCode()=97 -> bin 1 (16 buckets), "b"=98 -> bin 2, "q"=113 -> bin 1 (113 & 15 = 1): chained after "a"
        let mut m = JavaHashMap::new();
        for k in ["q", "b", "a"] {
            m.insert(k.to_owned(), ());
        }
        assert_eq!(m.keys().map(String::as_str).collect::<Vec<_>>(), ["q", "a", "b"]);
    }

    #[test]
    fn resize_keeps_chain_order_and_replace_keeps_position() {
        let mut m = JavaHashMap::new();
        for i in 0..13 {
            m.insert(format!("k{i}"), i);
        }
        assert_eq!(m.len(), 13);
        let before: Vec<String> = m.keys().cloned().collect();
        assert_eq!(m.insert("k3".to_owned(), 99), Some(3));
        assert_eq!(m.keys().cloned().collect::<Vec<_>>(), before);
    }
}
