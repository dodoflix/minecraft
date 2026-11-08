//! Iteration order of the hash-based Java collections vanilla encodes/iterates, for keys whose `hashCode()` is stable
//! (`String`, `Identifier`, `Integer`, ...). Keys hashed by `Object.hashCode()` (registry holders, `DataComponentType`,
//! `ResourceKey`, `TagKey`, enums) iterate in a per-JVM-run random order and cannot be modelled.
//!
//! Tested against the real `java.util.HashMap` / fastutil classes (`tests/golden/java_collections.rs`, `HashOrderGolden.java`).

use crate::math::java::java_string_hash;
use crate::pos::BlockPos;
use std::fmt;

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
pub fn hash_map_ordered<T>(
    items: Vec<T>,
    hash: impl Fn(&T) -> i32,
    initial_capacity: usize,
) -> Vec<T> {
    let hashes: Vec<i32> = items.iter().map(&hash).collect();
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    hash_map_order(&hashes, initial_capacity)
        .into_iter()
        .filter_map(|i| slots[i].take())
        .collect()
}

/// fastutil `HashCommon.mix(int)`.
fn mix(x: i32) -> usize {
    let h = (x as u32).wrapping_mul(0x9E37_79B9);
    (h ^ (h >> 16)) as usize
}

/// fastutil `Object2ObjectOpenHashMap` / `Int2ObjectOpenHashMap` / `ObjectOpenHashSet` (load factor 0.75, linear
/// probing, backward-shift deletion, shrink on remove): [`FastOpenHashMap::iter`] is the iteration order of vanilla's map
/// after the same sequence of `put`s, `remove`s and `clear`s. `hash` is the key's `hashCode()`. The key fastutil keeps
/// outside the table (`null` for object maps, `0` for int maps) is iterated first.
#[derive(Clone)]
pub struct FastOpenHashMap<K, V> {
    null_key: Option<(K, V)>,
    is_null: fn(&K) -> bool,
    slots: Vec<Option<(K, V)>>,
    mask: usize,
    max_fill: usize,
    /// `minN`: the table never shrinks below the size of the constructor's `expected`.
    min_n: usize,
    size: usize,
    hash: fn(&K) -> i32,
}

impl<K: PartialEq + fmt::Debug, V: fmt::Debug> fmt::Debug for FastOpenHashMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: PartialEq, V> FastOpenHashMap<K, V> {
    /// `HashCommon.arraySize(expected, 0.75F)`.
    fn array_size(expected: usize) -> usize {
        ((expected as f32 / 0.75f32).ceil() as u64)
            .next_power_of_two()
            .max(2) as usize
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
        FastOpenHashMap {
            null_key: None,
            is_null,
            slots: (0..n).map(|_| None).collect(),
            mask: n - 1,
            max_fill: Self::max_fill(n),
            min_n: n,
            size: 0,
            hash,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// The table slot holding `k` (never the null key).
    fn find(&self, k: &K) -> Option<usize> {
        let mut pos = mix((self.hash)(k)) & self.mask;
        while let Some((ek, _)) = &self.slots[pos] {
            if ek == k {
                return Some(pos);
            }
            pos = (pos + 1) & self.mask;
        }
        None
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        if (self.is_null)(k) {
            return self.null_key.as_ref().map(|e| &e.1);
        }
        self.find(k)
            .and_then(|pos| self.slots[pos].as_ref())
            .map(|e| &e.1)
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        if (self.is_null)(k) {
            return self.null_key.as_mut().map(|e| &mut e.1);
        }
        let pos = self.find(k)?;
        self.slots[pos].as_mut().map(|e| &mut e.1)
    }

    pub fn contains_key(&self, k: &K) -> bool {
        self.get(k).is_some()
    }

    /// `put`: an existing key keeps its slot and gets the new value (the old one is returned).
    pub fn put(&mut self, k: K, v: V) -> Option<V> {
        if (self.is_null)(&k) {
            if let Some(e) = &mut self.null_key {
                return Some(std::mem::replace(&mut e.1, v));
            }
            self.null_key = Some((k, v));
            self.grow();
            return None;
        }
        let mut pos = mix((self.hash)(&k)) & self.mask;
        while let Some((ek, ev)) = &mut self.slots[pos] {
            if *ek == k {
                return Some(std::mem::replace(ev, v));
            }
            pos = (pos + 1) & self.mask;
        }
        self.slots[pos] = Some((k, v));
        self.grow();
        None
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

    /// `remove(k)`: backward-shift deletion, then the table halves while it is under a quarter of `maxFill`.
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let removed = if (self.is_null)(k) {
            self.null_key.take()?.1
        } else {
            let pos = self.find(k)?;
            let removed = self.slots[pos].take()?.1;
            self.shift_keys(pos);
            removed
        };
        self.size -= 1;
        let n = self.mask + 1;
        if n > self.min_n && self.size < self.max_fill / 4 && n > 16 {
            self.rehash(n / 2);
        }
        Some(removed)
    }

    /// `shiftKeys(pos)`: refills the hole at `pos` from the probe sequence behind it.
    fn shift_keys(&mut self, mut pos: usize) {
        loop {
            let last = pos;
            pos = (last + 1) & self.mask;
            loop {
                let Some((ck, _)) = &self.slots[pos] else {
                    return;
                };
                let slot = mix((self.hash)(ck)) & self.mask;
                let movable = if last <= pos {
                    last >= slot || slot > pos
                } else {
                    last >= slot && slot > pos
                };
                if movable {
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            self.slots[last] = self.slots[pos].take();
        }
    }

    /// `clear()`: the table keeps its size.
    pub fn clear(&mut self) {
        self.null_key = None;
        self.slots.iter_mut().for_each(|s| *s = None);
        self.size = 0;
    }

    /// The map iterator walks the table from its last slot down.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.null_key
            .iter()
            .chain(self.slots.iter().rev().flatten())
            .map(|e| (&e.0, &e.1))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.null_key
            .iter_mut()
            .chain(self.slots.iter_mut().rev().flatten())
            .map(|e| (&e.0, &mut e.1))
    }

    pub fn into_entries(self) -> Vec<(K, V)> {
        self.null_key
            .into_iter()
            .chain(self.slots.into_iter().rev().flatten())
            .collect()
    }
}

/// `java.util.HashMap` with its table, bin order and resize policy (load factor 0.75, never shrinks): iteration is
/// ascending bucket, insertion order within a bucket (a resize keeps the relative order). Bins of 9+ keys in a table
/// of 64+ are not treeified here (treeifying reorders them).
#[derive(Clone)]
pub struct JavaHashMap<K, V> {
    /// Empty until the first `put` (Java allocates the table lazily).
    table: Vec<Vec<(K, V)>>,
    /// `threshold` before the table exists: the initial capacity (0: the default 16).
    initial_capacity: usize,
    size: usize,
    hash: fn(&K) -> i32,
}

impl<K: PartialEq + fmt::Debug, V: fmt::Debug> fmt::Debug for JavaHashMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: PartialEq, V> JavaHashMap<K, V> {
    /// `new HashMap<>()`.
    pub fn new(hash: fn(&K) -> i32) -> Self {
        Self::with_capacity(0, hash)
    }

    /// `new HashMap<>(initialCapacity)` (`0` is the default table of 16).
    pub fn with_capacity(initial_capacity: usize, hash: fn(&K) -> i32) -> Self {
        JavaHashMap {
            table: Vec::new(),
            initial_capacity: if initial_capacity == 0 {
                0
            } else {
                table_size_for(initial_capacity)
            },
            size: 0,
            hash,
        }
    }

    fn bucket(&self, k: &K, cap: usize) -> usize {
        let h = (self.hash)(k);
        ((h ^ ((h as u32) >> 16) as i32) as u32 as usize) & (cap - 1)
    }

    /// `(int)(cap * 0.75F)`.
    fn threshold(cap: usize) -> usize {
        (cap as f32 * 0.75f32) as usize
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        if self.table.is_empty() {
            return None;
        }
        self.table[self.bucket(k, self.table.len())]
            .iter()
            .find(|e| e.0 == *k)
            .map(|e| &e.1)
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        if self.table.is_empty() {
            return None;
        }
        let b = self.bucket(k, self.table.len());
        self.table[b]
            .iter_mut()
            .find(|e| e.0 == *k)
            .map(|e| &mut e.1)
    }

    pub fn contains_key(&self, k: &K) -> bool {
        self.get(k).is_some()
    }

    /// `put`: an existing key keeps its place and gets the new value (the old one is returned).
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        if self.table.is_empty() {
            let cap = if self.initial_capacity == 0 {
                HASH_MAP_DEFAULT_CAPACITY
            } else {
                self.initial_capacity
            };
            self.table = (0..cap).map(|_| Vec::new()).collect();
        }
        let b = self.bucket(&k, self.table.len());
        if let Some(e) = self.table[b].iter_mut().find(|e| e.0 == k) {
            return Some(std::mem::replace(&mut e.1, v));
        }
        self.table[b].push((k, v));
        // `treeifyBin`: a bin that reaches 9 keys in a table under 64 doubles the table instead of treeifying
        if self.table[b].len() > 8 && self.table.len() < 64 {
            self.resize();
        }
        self.size += 1;
        if self.size > Self::threshold(self.table.len()) {
            self.resize();
        }
        None
    }

    /// `resize()`: double the table; entries are re-bucketed in iteration order, which keeps each bin's order.
    fn resize(&mut self) {
        let cap = self.table.len() * 2;
        let old = std::mem::take(&mut self.table);
        self.table = (0..cap).map(|_| Vec::new()).collect();
        for e in old.into_iter().flatten() {
            let b = self.bucket(&e.0, cap);
            self.table[b].push(e);
        }
    }

    /// `putAll(m)` (`putMapEntries`): a map without a table is pre-sized for `m.size()` entries, one with a table
    /// doubles while `m.size()` exceeds its threshold.
    pub fn put_all(&mut self, entries: impl ExactSizeIterator<Item = (K, V)>) {
        let s = entries.len();
        if s == 0 {
            return;
        }
        if self.table.is_empty() {
            let t = calculate_hash_map_capacity(s);
            if t > self.initial_capacity {
                self.initial_capacity = table_size_for(t);
            }
        } else {
            while s > Self::threshold(self.table.len()) {
                self.resize();
            }
        }
        for (k, v) in entries {
            self.insert(k, v);
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        if self.table.is_empty() {
            return None;
        }
        let b = self.bucket(k, self.table.len());
        let i = self.table[b].iter().position(|e| e.0 == *k)?;
        self.size -= 1;
        Some(self.table[b].remove(i).1)
    }

    /// `clear()`: the table keeps its size.
    pub fn clear(&mut self) {
        self.table.iter_mut().for_each(Vec::clear);
        self.size = 0;
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.table.iter().flatten().map(|e| (&e.0, &e.1))
    }

    pub fn into_entries(self) -> Vec<(K, V)> {
        self.table.into_iter().flatten().collect()
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|e| e.0)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|e| e.1)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.table.iter_mut().flatten().map(|e| &mut e.1)
    }
}

impl<'a, K: PartialEq, V> IntoIterator for &'a JavaHashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Box<dyn Iterator<Item = (&'a K, &'a V)> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

/// `BlockPos.hashCode()`: `(y + z * 31) * 31 + x`.
pub fn block_pos_hash(p: &BlockPos) -> i32 {
    p.hash_code()
}

/// The keys of `new HashSet<>(first)` after `addAll(second)` in iteration order (vanilla `getBlockEntitiesPos`).
pub fn hash_set_union_order<K: PartialEq + Clone>(
    first: impl ExactSizeIterator<Item = K>,
    second: impl Iterator<Item = K>,
    hash: fn(&K) -> i32,
) -> Vec<K> {
    let mut set = JavaHashMap::with_capacity(hash_set_copy_capacity(first.len()), hash);
    first.chain(second).for_each(|k| {
        set.insert(k, ());
    });
    set.keys().cloned().collect()
}

/// `FastOpenHashMap` keyed by `String`.
pub fn fast_open_hash_order_string<V>(
    expected: usize,
    entries: impl IntoIterator<Item = (String, V)>,
) -> Vec<(String, V)> {
    let mut m = FastOpenHashMap::new(expected, |k: &String| java_string_hash(k));
    for (k, v) in entries {
        m.put(k, v);
    }
    m.into_entries()
}
