//! Iteration order of the hash-based Java collections vanilla encodes/iterates, for keys whose `hashCode()` is stable
//! (`String`, `Identifier`, `Integer`, ...). Keys hashed by `Object.hashCode()` (registry holders, `DataComponentType`,
//! `ResourceKey`, `TagKey`, enums) iterate in a per-JVM-run random order and cannot be modelled.
//!
//! Tested against the real `java.util.HashMap` / fastutil classes (`tests/java_collections.rs`, `HashOrderGolden.java`).

use crate::math::java::java_string_hash;

/// `java.util.HashMap.tableSizeFor(cap)`.
fn table_size_for(cap: usize) -> usize {
    cap.max(1).next_power_of_two()
}

/// `HashMap.calculateHashMapCapacity(n)` (JDK 19+): `ceil(n / 0.75)`.
fn calculate_hash_map_capacity(n: usize) -> usize {
    (n as f64 / 0.75f64).ceil() as usize
}

/// Initial table size of `new HashMap<>(map)` for `size` entries (`HashMap.putMapEntries`, JDK 19+: `ceil(size / 0.75)`
/// rounded up to a power of two; before JDK 19 it was `size / 0.75F + 1.0F`).
pub fn hash_map_copy_capacity(size: usize) -> usize {
    table_size_for(calculate_hash_map_capacity(size))
}

/// Initial table size of `new HashSet<>(collection)` (JDK 19+: `calculateHashMapCapacity(max(size, 12))`).
pub fn hash_set_copy_capacity(size: usize) -> usize {
    table_size_for(calculate_hash_map_capacity(size.max(12)))
}

/// Default `HashMap` table size.
pub const HASH_MAP_DEFAULT_CAPACITY: usize = 16;

/// Indices of `hashes` (the `hashCode()` of distinct keys, in `put` order) in `java.util.HashMap`/`HashSet` iteration
/// order: ascending bucket (`h ^ h >>> 16` masked to the table size), insertion order within a bucket. The table starts
/// at `initial_capacity` (a power of two; `HASH_MAP_DEFAULT_CAPACITY` for `new HashMap<>()`) and doubles while the size
/// exceeds 0.75 of it. Treeified bins (8+ colliding keys in a 64+ table) reorder their members and are not modelled.
pub fn hash_map_order(hashes: &[i32], initial_capacity: usize) -> Vec<usize> {
    let mut cap = table_size_for(initial_capacity);
    while hashes.len() > (cap as f32 * 0.75f32) as usize {
        cap *= 2;
    }
    let bucket = |h: i32| ((h ^ ((h as u32) >> 16) as i32) as u32 as usize) & (cap - 1);
    let mut idx: Vec<usize> = (0..hashes.len()).collect();
    idx.sort_by_key(|&i| bucket(hashes[i])); // stable: insertion order within a bucket
    idx
}

/// `items` (distinct keys, in `put` order) reordered into `java.util.HashMap` iteration order, see [`hash_map_order`].
pub fn hash_map_ordered<T>(items: Vec<T>, hash: impl Fn(&T) -> i32, initial_capacity: usize) -> Vec<T> {
    let hashes: Vec<i32> = items.iter().map(&hash).collect();
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    hash_map_order(&hashes, initial_capacity).into_iter().filter_map(|i| slots[i].take()).collect()
}

/// fastutil `HashCommon.mix(int)`.
fn mix(x: i32) -> usize {
    let h = (x as u32).wrapping_mul(0x9E37_79B9);
    (h ^ (h >> 16)) as usize
}

/// fastutil `Object2ObjectOpenHashMap` / `Int2ObjectOpenHashMap` / `ObjectOpenHashSet` (load factor 0.75, linear
/// probing) with `put` only: [`FastOpenHashMap::into_entries`] is the iteration order of vanilla's map after the same
/// sequence of `put`s. `hash` is the key's `hashCode()`. The key fastutil keeps outside the table (`null` for object
/// maps, `0` for int maps) is iterated first.
pub struct FastOpenHashMap<K, V> {
    null_key: Option<(K, V)>,
    is_null: fn(&K) -> bool,
    slots: Vec<Option<(K, V)>>,
    mask: usize,
    max_fill: usize,
    size: usize,
    hash: fn(&K) -> i32,
}

impl<K: PartialEq, V> FastOpenHashMap<K, V> {
    /// `HashCommon.arraySize(expected, 0.75F)`.
    fn array_size(expected: usize) -> usize {
        ((expected as f32 / 0.75f32).ceil() as u64).next_power_of_two().max(2) as usize
    }

    /// `HashCommon.maxFill(n, 0.75F)`.
    fn max_fill(n: usize) -> usize {
        ((n as f32 * 0.75f32).ceil() as usize).min(n - 1)
    }

    /// `new Object2ObjectOpenHashMap<>(expected)` (`new ...OpenHashMap<>()` is `expected = 16`); Java `null` keys are not
    /// modelled (a Rust key is never null).
    pub fn new(expected: usize, hash: fn(&K) -> i32) -> Self {
        Self::with_null_key(expected, hash, |_| false)
    }

    /// As [`Self::new`]; `is_null` selects the key stored outside the table (`k == 0` for fastutil's `Int2...` maps).
    pub fn with_null_key(expected: usize, hash: fn(&K) -> i32, is_null: fn(&K) -> bool) -> Self {
        let n = Self::array_size(expected);
        FastOpenHashMap { null_key: None, is_null, slots: (0..n).map(|_| None).collect(), mask: n - 1, max_fill: Self::max_fill(n), size: 0, hash }
    }

    pub fn put(&mut self, k: K, v: V) {
        if (self.is_null)(&k) {
            if let Some(e) = &mut self.null_key {
                e.1 = v;
                return;
            }
            self.null_key = Some((k, v));
            self.grow();
            return;
        }
        let mut pos = mix((self.hash)(&k)) & self.mask;
        while let Some((ek, ev)) = &mut self.slots[pos] {
            if *ek == k {
                *ev = v;
                return;
            }
            pos = (pos + 1) & self.mask;
        }
        self.slots[pos] = Some((k, v));
        self.grow();
    }

    /// `if (size++ >= maxFill) rehash(arraySize(size + 1, f))`.
    fn grow(&mut self) {
        let old = self.size;
        self.size += 1;
        if old >= self.max_fill {
            self.rehash(Self::array_size(self.size + 1));
        }
    }

    /// `rehash`: entries are re-inserted scanning the old table from its end.
    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_slots: Vec<Option<(K, V)>> = (0..new_n).map(|_| None).collect();
        for entry in self.slots.iter_mut().rev().filter_map(Option::take) {
            let mut pos = mix((self.hash)(&entry.0)) & mask;
            while new_slots[pos].is_some() {
                pos = (pos + 1) & mask;
            }
            new_slots[pos] = Some(entry);
        }
        self.slots = new_slots;
        self.mask = mask;
        self.max_fill = Self::max_fill(new_n);
    }

    /// The map iterator walks the table from its last slot down.
    pub fn into_entries(self) -> Vec<(K, V)> {
        self.null_key.into_iter().chain(self.slots.into_iter().rev().flatten()).collect()
    }
}

/// `FastOpenHashMap` keyed by `String`.
pub fn fast_open_hash_order_string<V>(expected: usize, entries: impl IntoIterator<Item = (String, V)>) -> Vec<(String, V)> {
    let mut m = FastOpenHashMap::new(expected, |k: &String| java_string_hash(k));
    for (k, v) in entries {
        m.put(k, v);
    }
    m.into_entries()
}
