//! Iteration orders vanilla's statistics code exposes: `Object2IntOpenHashMap` (the counter itself and the
//! `ClientboundAwardStatsPacket` map) and `java.util.HashMap` (`Util.toMap()` groups, dirty set) keyed by
//! `Stat.hashCode()` = `getName().hashCode()`.
use mc_core::math::java::java_string_hash;

/// `HashCommon.mix(int)`.
fn mix(x: i32) -> i32 {
    let h = x.wrapping_mul(0x9E37_79B9_u32 as i32);
    h ^ ((h as u32) >> 16) as i32
}

/// fastutil `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    ((expected as f64 / 0.75).ceil() as usize)
        .next_power_of_two()
        .max(2)
}

/// fastutil `HashCommon.maxFill(n, 0.75f)`.
fn max_fill(n: usize) -> usize {
    ((n as f64 * 0.75).ceil() as usize).min(n - 1)
}

/// fastutil `tryCapacity(capacity)` presize of a `0.75` load factor map whose table has `n` slots:
/// the table size after `putAll` of `capacity` entries (never shrinks).
pub fn presized_table(n: usize, capacity: usize) -> usize {
    let needed = ((capacity as f64 / 0.75).ceil() as usize)
        .next_power_of_two()
        .max(2);
    needed.max(n)
}

/// The default table size of `new Object2IntOpenHashMap()` (`expected = 16`).
pub const DEFAULT_TABLE: usize = 32;

/// Iteration order (indices into `hashes`) of an `Object2IntOpenHashMap` with a table of `n0` slots (a power of two,
/// [`DEFAULT_TABLE`] for the default constructor) built by `put`ting keys with these hash codes in this order
/// (distinct keys).
pub fn object_open_hash_order(hashes: &[i32], n0: usize) -> Vec<usize> {
    let mut n = n0;
    let mut max = max_fill(n);
    let mut table: Vec<Option<usize>> = vec![None; n];
    let place = |table: &mut Vec<Option<usize>>, mask: usize, idx: usize| {
        let mut pos = (mix(hashes[idx]) as u32 as usize) & mask;
        while table[pos].is_some() {
            pos = (pos + 1) & mask;
        }
        table[pos] = Some(idx);
    };
    let mut size = 0;
    for idx in 0..hashes.len() {
        place(&mut table, n - 1, idx);
        let grow = size >= max;
        size += 1;
        if grow {
            let new_n = array_size(size + 1);
            let mut new_table = vec![None; new_n];
            // rehash walks the old table from the top
            for slot in table.iter().rev().flatten() {
                place(&mut new_table, new_n - 1, *slot);
            }
            table = new_table;
            n = new_n;
            max = max_fill(n);
        }
    }
    table.iter().rev().flatten().copied().collect()
}

/// `java.util.HashMap` bookkeeping (table size, threshold, bin order) for stats' `HashMap<Stat, Integer>`s; the entries
/// are indices (the caller keeps the payload) with their `hashCode`.
#[derive(Default)]
pub struct JavaHashMapSim {
    cap: usize,
    threshold: usize,
    bins: Vec<Vec<(usize, i32)>>,
    size: usize,
}

impl JavaHashMapSim {
    /// `new HashMap<>()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `new HashMap<>(initialCapacity)`.
    pub fn with_capacity(initial: usize) -> Self {
        Self {
            threshold: initial.max(1).next_power_of_two(),
            ..Self::default()
        }
    }

    fn resize(&mut self) {
        let (old_cap, old_thr) = (self.cap, self.threshold);
        let (new_cap, mut new_thr) = if old_cap > 0 {
            (old_cap * 2, if old_cap >= 16 { old_thr * 2 } else { 0 })
        } else if old_thr > 0 {
            (old_thr, 0)
        } else {
            (16, 12)
        };
        if new_thr == 0 {
            new_thr = (new_cap as f32 * 0.75) as usize;
        }
        self.cap = new_cap;
        self.threshold = new_thr;
        let old = std::mem::take(&mut self.bins);
        self.bins = vec![Vec::new(); new_cap];
        for (i, h) in old.into_iter().flatten() {
            self.bins[(h as u32 as usize) & (new_cap - 1)].push((i, h));
        }
    }

    /// The presizing step of `putAll(map)` (`putMapEntries`) for a map of `s` entries.
    pub fn put_all_start(&mut self, s: usize) {
        if s == 0 {
            return;
        }
        if self.cap == 0 {
            let t = (s as f32 / 0.75 + 1.0) as usize;
            if t > self.threshold {
                self.threshold = t.next_power_of_two();
            }
        } else {
            while s > self.threshold {
                self.resize();
            }
        }
    }

    /// `put` of a new key.
    pub fn put(&mut self, idx: usize, hash: i32) {
        if self.cap == 0 {
            self.resize();
        }
        let spread = hash ^ ((hash as u32) >> 16) as i32;
        self.bins[(spread as u32 as usize) & (self.cap - 1)].push((idx, spread));
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
    }

    /// Iteration order (the indices).
    pub fn order(&self) -> Vec<usize> {
        self.bins.iter().flatten().map(|(i, _)| *i).collect()
    }
}

/// Iteration order (indices into `names`) of a `java.util.HashMap` with these keys (hash = `String.hashCode`)
/// inserted in this order (`initial_capacity` as the constructor argument; 16 for `new HashMap<>()`).
pub fn java_hash_map_order(names: &[String], initial_capacity: usize) -> Vec<usize> {
    let mut map = if initial_capacity == 16 {
        JavaHashMapSim::new()
    } else {
        JavaHashMapSim::with_capacity(initial_capacity)
    };
    for (i, name) in names.iter().enumerate() {
        map.put(i, java_string_hash(name));
    }
    map.order()
}
