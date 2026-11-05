//! Port of fastutil 8.5 `Long2ObjectOpenHashMap` / `LongOpenHashSet` (open addressing, linear probing, load
//! factor 0.75, backwards iteration with the `0` key first). The *iteration order* is observable in vanilla
//! (chunk tick order, spawn candidate shuffles, unload order), so the table layout is reproduced exactly:
//! same hash mix, same growth/shrink thresholds, same `shiftKeys` on removal. Port of the table logic of
//! `mc_world::ticks::java_util::Long2LongOpenHashMap`, generalised over the value type.

const LONG_PHI: i64 = 0x9E37_79B9_7F4A_7C15u64 as i64;

/// `HashCommon.mix(long)` truncated to `int`.
fn mix(x: i64) -> i32 {
    let mut h = x.wrapping_mul(LONG_PHI);
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32
}

/// `HashCommon.mix(int)`.
fn mix_int(x: i64) -> i32 {
    let h = (x as i32).wrapping_mul(0x9E37_79B9u32 as i32);
    h ^ (((h as u32) >> 16) as i32)
}

fn next_power_of_two(x: u64) -> u64 {
    if x == 0 { 1 } else { (x - 1).next_power_of_two().max(1) }
}

/// `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    next_power_of_two(((expected as f64) / 0.75).ceil() as u64).max(2) as usize
}

/// `HashCommon.maxFill(n, 0.75f)`.
fn max_fill(n: usize) -> usize {
    (((n as f64) * 0.75).ceil() as usize).min(n - 1)
}

const DEFAULT_INITIAL_SIZE: usize = 16;

/// `Long2ObjectOpenHashMap<V>` (also used for `Long2ByteOpenHashMap` with `V = u8`).
pub struct LongOpenHashMap<V> {
    key: Vec<i64>,
    value: Vec<Option<V>>,
    mask: usize,
    contains_null_key: bool,
    /// `Int2ObjectOpenHashMap`: keys are ints (hashed with `mix(int)`).
    int_keys: bool,
    n: usize,
    min_n: usize,
    max_fill: usize,
    size: usize,
}

impl<V> Default for LongOpenHashMap<V> {
    fn default() -> Self {
        Self::with_expected(DEFAULT_INITIAL_SIZE)
    }
}

impl<V> LongOpenHashMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `new Long2ObjectOpenHashMap<>(expected)` (load factor 0.75).
    pub fn with_expected(expected: usize) -> Self {
        let n = array_size(expected);
        let mut value = Vec::new();
        value.resize_with(n + 1, || None);
        Self { key: vec![0; n + 1], value, mask: n - 1, contains_null_key: false, int_keys: false, n, min_n: n, max_fill: max_fill(n), size: 0 }
    }

    /// `new Int2ObjectOpenHashMap<>()`: same table, int keys.
    pub fn new_int() -> Self {
        Self { int_keys: true, ..Self::new() }
    }

    fn hash(&self, k: i64) -> usize {
        (if self.int_keys { mix_int(k) } else { mix(k) }) as usize
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn find(&self, k: i64) -> Result<usize, usize> {
        if k == 0 {
            return if self.contains_null_key { Ok(self.n) } else { Err(self.n) };
        }
        let mut pos = self.hash(k) & self.mask;
        loop {
            let curr = self.key[pos];
            if curr == 0 {
                return Err(pos);
            }
            if curr == k {
                return Ok(pos);
            }
            pos = (pos + 1) & self.mask;
        }
    }

    pub fn contains_key(&self, k: i64) -> bool {
        self.find(k).is_ok()
    }

    pub fn get(&self, k: i64) -> Option<&V> {
        self.find(k).ok().and_then(|p| self.value[p].as_ref())
    }

    pub fn get_mut(&mut self, k: i64) -> Option<&mut V> {
        self.find(k).ok().and_then(|p| self.value[p].as_mut())
    }

    /// `put`: returns the previous value.
    pub fn insert(&mut self, k: i64, v: V) -> Option<V> {
        match self.find(k) {
            Ok(pos) => self.value[pos].replace(v),
            Err(pos) => {
                if pos == self.n {
                    self.contains_null_key = true;
                }
                self.key[pos] = k;
                self.value[pos] = Some(v);
                let old = self.size;
                self.size += 1;
                if old >= self.max_fill {
                    self.rehash(array_size(self.size + 1));
                }
                None
            }
        }
    }

    /// `computeIfAbsent(k, |_| make())`.
    pub fn get_or_insert_with(&mut self, k: i64, make: impl FnOnce() -> V) -> &mut V {
        if !self.contains_key(k) {
            self.insert(k, make());
        }
        self.get_mut(k).expect("just inserted")
    }

    pub fn remove(&mut self, k: i64) -> Option<V> {
        match self.find(k) {
            Err(_) => None,
            Ok(pos) if pos == self.n && k == 0 => {
                self.contains_null_key = false;
                let old = self.value[self.n].take();
                self.size -= 1;
                self.maybe_shrink();
                old
            }
            Ok(pos) => {
                let old = self.value[pos].take();
                self.size -= 1;
                self.shift_keys(pos, None);
                self.maybe_shrink();
                old
            }
        }
    }

    pub fn clear(&mut self) {
        if self.size == 0 {
            return;
        }
        self.size = 0;
        self.contains_null_key = false;
        self.key.iter_mut().for_each(|k| *k = 0);
        self.value.iter_mut().for_each(|v| *v = None);
    }

    fn maybe_shrink(&mut self) {
        if self.n > self.min_n && self.size < self.max_fill / 4 && self.n > DEFAULT_INITIAL_SIZE {
            self.rehash(self.n / 2);
        }
    }

    /// `shiftKeys`; with `wrapped` set this is the iterator's variant that records entries moved across the array end.
    fn shift_keys(&mut self, mut pos: usize, mut wrapped: Option<&mut Vec<i64>>) {
        loop {
            let last = pos;
            pos = (pos + 1) & self.mask;
            let curr;
            loop {
                let c = self.key[pos];
                if c == 0 {
                    self.key[last] = 0;
                    self.value[last] = None;
                    return;
                }
                let slot = self.hash(c) & self.mask;
                let stays = if last <= pos { last >= slot || slot > pos } else { last >= slot && slot > pos };
                if stays {
                    curr = c;
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            if pos < last {
                if let Some(w) = wrapped.as_deref_mut() {
                    w.push(self.key[pos]);
                }
            }
            self.key[last] = curr;
            self.value[last] = self.value[pos].take();
        }
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key = vec![0i64; new_n + 1];
        let mut new_value: Vec<Option<V>> = Vec::new();
        new_value.resize_with(new_n + 1, || None);
        let mut i = self.n;
        let mut j = if self.contains_null_key { self.size - 1 } else { self.size };
        while j > 0 {
            j -= 1;
            loop {
                i -= 1;
                if self.key[i] != 0 {
                    break;
                }
            }
            let mut pos = self.hash(self.key[i]) & mask;
            while new_key[pos] != 0 {
                pos = (pos + 1) & mask;
            }
            new_key[pos] = self.key[i];
            new_value[pos] = self.value[i].take();
        }
        new_value[new_n] = self.value[self.n].take();
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n);
        self.key = new_key;
        self.value = new_value;
    }

    /// Keys in fastutil iteration order (`keySet().iterator()` / `fastIterator`): `0` key first, then the table
    /// from the last slot down.
    pub fn keys(&self) -> Vec<i64> {
        let mut out = Vec::with_capacity(self.size);
        if self.contains_null_key {
            out.push(0);
        }
        for pos in (0..self.n).rev() {
            if self.key[pos] != 0 {
                out.push(self.key[pos]);
            }
        }
        out
    }

    /// Entries in fastutil iteration order.
    pub fn iter(&self) -> impl Iterator<Item = (i64, &V)> {
        let null = self.contains_null_key.then(|| (0i64, self.value[self.n].as_ref().expect("null key value")));
        null.into_iter().chain((0..self.n).rev().filter(|&p| self.key[p] != 0).map(|p| (self.key[p], self.value[p].as_ref().expect("value"))))
    }

    /// `values()` in iteration order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// Iterator-with-`remove()` loop over the entries in fastutil order: `f(key, &mut value)` returns `true` to
    /// remove the entry just visited.
    pub fn drain_filter_in_order(&mut self, mut f: impl FnMut(i64, &mut V) -> bool) {
        let mut c = self.size;
        let mut pos = self.n as isize;
        let mut must_return_null_key = self.contains_null_key;
        let mut wrapped: Vec<i64> = Vec::new();
        while c != 0 {
            c -= 1;
            let (index, last_is_null, last_is_wrapped);
            if must_return_null_key {
                must_return_null_key = false;
                index = self.n;
                last_is_null = true;
                last_is_wrapped = false;
            } else {
                loop {
                    pos -= 1;
                    if pos < 0 {
                        let k = wrapped[(-pos - 1) as usize];
                        let mut p = self.hash(k) & self.mask;
                        while k != self.key[p] {
                            p = (p + 1) & self.mask;
                        }
                        index = p;
                        last_is_null = false;
                        last_is_wrapped = true;
                        break;
                    }
                    if self.key[pos as usize] != 0 {
                        index = pos as usize;
                        last_is_null = false;
                        last_is_wrapped = false;
                        break;
                    }
                }
            }
            let key = if last_is_null { 0 } else { self.key[index] };
            let remove = f(key, self.value[index].as_mut().expect("value"));
            if remove {
                if last_is_null {
                    self.contains_null_key = false;
                    self.value[self.n] = None;
                    self.size -= 1;
                } else if !last_is_wrapped {
                    self.value[index] = None;
                    self.shift_keys(index, Some(&mut wrapped));
                    self.size -= 1;
                } else {
                    let k = wrapped[(-pos - 1) as usize];
                    self.remove(k);
                }
            }
        }
    }
}

/// `LongOpenHashSet`.
#[derive(Default)]
pub struct LongOpenHashSet(LongOpenHashMap<()>);

impl LongOpenHashSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, k: i64) -> bool {
        self.0.contains_key(k)
    }

    /// `add`: whether the key was absent.
    pub fn add(&mut self, k: i64) -> bool {
        self.0.insert(k, ()).is_none()
    }

    /// `remove`: whether the key was present.
    pub fn remove(&mut self, k: i64) -> bool {
        self.0.remove(k).is_some()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Keys in iteration order.
    pub fn iter(&self) -> impl Iterator<Item = i64> {
        self.0.keys().into_iter()
    }

    /// Iterator loop with `remove()`: `f` returns `true` to remove the key just visited.
    pub fn drain_filter_in_order(&mut self, mut f: impl FnMut(i64) -> bool) {
        self.0.drain_filter_in_order(|k, _| f(k));
    }
}

/// fastutil 8.5 `ShortOpenHashSet` (default constructor: 16 expected entries, load factor 0.75): insertion and the
/// iteration order (`0` key first, then the table scanned backwards), which `ChunkHolder.broadcastChanges` exposes in
/// `ClientboundSectionBlocksUpdatePacket`.
#[derive(Clone, Debug)]
pub struct ShortOpenHashSet {
    key: Vec<i16>,
    mask: usize,
    n: usize,
    max_fill: usize,
    contains_null: bool,
    size: usize,
}

fn mix_short(k: i16) -> usize {
    let h = i32::from(k).wrapping_mul(0x9E37_79B9u32 as i32);
    (h ^ ((h as u32) >> 16) as i32) as u32 as usize
}

impl Default for ShortOpenHashSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortOpenHashSet {
    pub fn new() -> Self {
        let n = array_size(16);
        Self { key: vec![0; n + 1], mask: n - 1, n, max_fill: max_fill(n), contains_null: false, size: 0 }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn contains(&self, k: i16) -> bool {
        if k == 0 {
            return self.contains_null;
        }
        let mut pos = mix_short(k) & self.mask;
        loop {
            let curr = self.key[pos];
            if curr == 0 {
                return false;
            }
            if curr == k {
                return true;
            }
            pos = (pos + 1) & self.mask;
        }
    }

    /// `add(k)`: `false` if already present.
    pub fn add(&mut self, k: i16) -> bool {
        if k == 0 {
            if self.contains_null {
                return false;
            }
            self.contains_null = true;
        } else {
            let mut pos = mix_short(k) & self.mask;
            while self.key[pos] != 0 {
                if self.key[pos] == k {
                    return false;
                }
                pos = (pos + 1) & self.mask;
            }
            self.key[pos] = k;
        }
        let was = self.size;
        self.size += 1;
        if was >= self.max_fill {
            self.rehash(array_size(self.size + 1));
        }
        true
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key = vec![0i16; new_n + 1];
        let mut i = self.n;
        let mut j = if self.contains_null { self.size - 1 } else { self.size };
        while j > 0 {
            j -= 1;
            loop {
                i -= 1;
                if self.key[i] != 0 {
                    break;
                }
            }
            let mut pos = mix_short(self.key[i]) & mask;
            while new_key[pos] != 0 {
                pos = (pos + 1) & mask;
            }
            new_key[pos] = self.key[i];
        }
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n);
        self.key = new_key;
    }

    /// Iteration order of the fastutil iterator.
    pub fn iter(&self) -> impl Iterator<Item = i16> + '_ {
        self.contains_null.then_some(0i16).into_iter().chain((0..self.n).rev().map(|p| self.key[p]).filter(|k| *k != 0))
    }
}

#[cfg(test)]
mod short_set_tests {
    use super::*;

    #[test]
    fn dedupes_and_iterates_zero_first() {
        let mut s = ShortOpenHashSet::new();
        assert!(s.add(5) && s.add(0) && s.add(9) && !s.add(5) && !s.add(0));
        assert_eq!(s.len(), 3);
        assert_eq!(s.iter().next(), Some(0));
        assert!(s.contains(9) && !s.contains(10));
    }

    #[test]
    fn survives_rehash() {
        let mut s = ShortOpenHashSet::new();
        for k in 0..200i16 {
            assert!(s.add(k * 7));
        }
        assert_eq!(s.len(), 200);
        let mut all: Vec<i16> = s.iter().collect();
        all.sort_unstable();
        assert_eq!(all, (0..200).map(|k| k * 7).collect::<Vec<i16>>());
    }
}

#[cfg(test)]
mod int_map_tests {
    use super::*;

    /// Vanilla `Int2ObjectOpenHashMap` (`ChunkMap.entityMap`) iteration order after scripted put/remove sequences
    /// (generator: `tests/fixtures/Int2ObjectOrderGen.java`).
    #[test]
    fn int2object_iteration_order_matches_fastutil() {
        let golden = include_str!("../../../tests/fixtures/int2object_order.txt");
        assert_eq!(golden.lines().count(), 40);
        for line in golden.lines() {
            let (ops, expected) = line.split_once(" | ").expect("script | order");
            let mut map = LongOpenHashMap::new_int();
            for op in ops.split(' ') {
                let key: i64 = op[1..].parse().unwrap();
                if op.starts_with('+') {
                    map.insert(key, ());
                } else {
                    map.remove(key);
                }
            }
            let got: Vec<String> = map.keys().iter().map(|k| k.to_string()).collect();
            assert_eq!(got.join(" "), expected, "script {ops}");
        }
    }
}

/// Port of the table logic of fastutil `Object2IntOpenHashMap<K>` for maps that only grow (`computeInt` adding to
/// a count): same hash mix, probing, growth and the backwards iteration order, which is observable when the entries are
/// processed in order (`SculkSpreader.updateCursors` emits one level event per charged position). `hash` is the key's
/// Java `hashCode()`.
pub struct Object2IntOpenHashMap<K> {
    key: Vec<Option<K>>,
    value: Vec<i32>,
    hash: fn(&K) -> i32,
    mask: usize,
    n: usize,
    max_fill: usize,
    size: usize,
}

impl<K: Eq> Object2IntOpenHashMap<K> {
    /// `new Object2IntOpenHashMap<>()` (expected 16 entries, load factor 0.75).
    pub fn new(hash: fn(&K) -> i32) -> Self {
        let n = array_size(DEFAULT_INITIAL_SIZE);
        let mut key = Vec::new();
        key.resize_with(n + 1, || None);
        Self { key, value: vec![0; n + 1], hash, mask: n - 1, n, max_fill: max_fill(n), size: 0 }
    }

    /// `computeInt(k, (k, old) -> (old == null ? 0 : old) + delta)`.
    pub fn add_to_value(&mut self, k: K, delta: i32) {
        let mut pos = mix_int(i64::from((self.hash)(&k))) as usize & self.mask;
        loop {
            match &self.key[pos] {
                None => break,
                Some(existing) if *existing == k => {
                    self.value[pos] = self.value[pos].wrapping_add(delta);
                    return;
                }
                Some(_) => pos = (pos + 1) & self.mask,
            }
        }
        self.key[pos] = Some(k);
        self.value[pos] = delta;
        let old = self.size;
        self.size += 1;
        if old >= self.max_fill {
            self.rehash(array_size(self.size + 1));
        }
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key: Vec<Option<K>> = Vec::new();
        new_key.resize_with(new_n + 1, || None);
        let mut new_value = vec![0; new_n + 1];
        let mut i = self.n;
        for _ in 0..self.size {
            loop {
                i -= 1;
                if self.key[i].is_some() {
                    break;
                }
            }
            let k = self.key[i].take().expect("slot checked above");
            let mut pos = mix_int(i64::from((self.hash)(&k))) as usize & mask;
            while new_key[pos].is_some() {
                pos = (pos + 1) & mask;
            }
            new_value[pos] = self.value[i];
            new_key[pos] = Some(k);
        }
        self.key = new_key;
        self.value = new_value;
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n);
    }

    /// The entries in iteration order (table slots from the end).
    pub fn into_entries(self) -> impl Iterator<Item = (K, i32)> {
        let Self { key, value, n, .. } = self;
        key.into_iter().zip(value).take(n).collect::<Vec<_>>().into_iter().rev().filter_map(|(k, v)| k.map(|k| (k, v)))
    }
}

#[cfg(test)]
mod object_map_tests {
    use super::*;

    #[test]
    fn sums_per_key_and_survives_rehash() {
        let mut map = Object2IntOpenHashMap::new(|k: &i32| *k);
        for k in 0..100 {
            map.add_to_value(k, 1);
            map.add_to_value(k, k);
        }
        let mut all: Vec<(i32, i32)> = map.into_entries().collect();
        all.sort_unstable();
        assert_eq!(all, (0..100).map(|k| (k, k + 1)).collect::<Vec<_>>());
    }
}
