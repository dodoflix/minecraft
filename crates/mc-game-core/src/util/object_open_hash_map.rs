//! fastutil 8.5 `Object2ObjectOpenHashMap<String, V>`: the same table layout (open addressing, linear probing,
//! `HashCommon.mix(hashCode)`, `shiftKeys` on removal, growth/shrink thresholds), so the iteration orders are Java's for
//! any put/remove history. `Scoreboard` keeps its objectives, score holders and teams in such maps and the command
//! outputs list them in iteration order.
//!
//! Two orders exist: the map/collection *iterator* and `forEach` visit the table backwards (`iter`), the
//! `spliterator` behind `stream()` visits it forwards (`iter_stream`).

use mc_core::math::java::java_string_hash;

const INT_PHI: i32 = 0x9E37_79B9u32 as i32;
const DEFAULT_INITIAL_SIZE: usize = 16;
const DEFAULT_LOAD_FACTOR: f32 = 0.75;

/// `HashCommon.mix(k.hashCode())`.
fn mix(key: &str) -> i32 {
    let h = java_string_hash(key).wrapping_mul(INT_PHI);
    h ^ (((h as u32) >> 16) as i32)
}

/// `HashCommon.arraySize(expected, f)`.
fn array_size(expected: usize, f: f32) -> usize {
    let n = (expected as f32 / f).ceil() as u64;
    (if n <= 1 {
        1
    } else {
        (n - 1).next_power_of_two()
    })
    .max(2) as usize
}

/// `HashCommon.maxFill(n, f)`.
fn max_fill(n: usize, f: f32) -> usize {
    ((n as f32 * f).ceil() as usize).min(n - 1)
}

#[derive(Debug, Clone)]
pub struct ObjectOpenHashMap<V> {
    key: Vec<Option<String>>,
    value: Vec<Option<V>>,
    mask: usize,
    n: usize,
    min_n: usize,
    max_fill: usize,
    size: usize,
    f: f32,
}

impl<V> Default for ObjectOpenHashMap<V> {
    fn default() -> Self {
        Self::with_expected(DEFAULT_INITIAL_SIZE, DEFAULT_LOAD_FACTOR)
    }
}

impl<V> ObjectOpenHashMap<V> {
    /// `new Object2ObjectOpenHashMap<>()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `new Object2ObjectOpenHashMap<>(expected, f)`.
    pub fn with_expected(expected: usize, f: f32) -> Self {
        let n = array_size(expected, f);
        ObjectOpenHashMap {
            key: vec![None; n + 1],
            value: std::iter::repeat_with(|| None).take(n + 1).collect(),
            mask: n - 1,
            n,
            min_n: n,
            max_fill: max_fill(n, f),
            size: 0,
            f,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `find(k)`: `Ok(pos)` when present, `Err(free slot)` otherwise.
    fn find(&self, k: &str) -> Result<usize, usize> {
        let mut pos = mix(k) as u32 as usize & self.mask;
        loop {
            match &self.key[pos] {
                None => return Err(pos),
                Some(curr) if curr == k => return Ok(pos),
                Some(_) => pos = (pos + 1) & self.mask,
            }
        }
    }

    pub fn contains_key(&self, k: &str) -> bool {
        self.find(k).is_ok()
    }

    pub fn get(&self, k: &str) -> Option<&V> {
        self.find(k).ok().and_then(|p| self.value[p].as_ref())
    }

    pub fn get_mut(&mut self, k: &str) -> Option<&mut V> {
        match self.find(k) {
            Ok(p) => self.value[p].as_mut(),
            Err(_) => None,
        }
    }

    /// `put(k, v)`: the previous value.
    pub fn insert(&mut self, k: String, v: V) -> Option<V> {
        match self.find(&k) {
            Ok(p) => self.value[p].replace(v),
            Err(p) => {
                self.key[p] = Some(k);
                self.value[p] = Some(v);
                let grow = self.size >= self.max_fill;
                self.size += 1;
                if grow {
                    self.rehash(array_size(self.size + 1, self.f));
                }
                None
            }
        }
    }

    /// `computeIfAbsent(k, ..)`.
    pub fn get_or_insert_with(&mut self, k: &str, make: impl FnOnce() -> V) -> &mut V {
        if !self.contains_key(k) {
            self.insert(k.to_owned(), make());
        }
        self.get_mut(k).expect("just inserted")
    }

    /// `remove(k)`.
    pub fn remove(&mut self, k: &str) -> Option<V> {
        let pos = self.find(k).ok()?;
        let old = self.value[pos].take();
        self.size -= 1;
        self.shift_keys(pos);
        if self.n > self.min_n && self.size < self.max_fill / 4 && self.n > DEFAULT_INITIAL_SIZE {
            self.rehash(self.n / 2);
        }
        old
    }

    fn shift_keys(&mut self, mut pos: usize) {
        loop {
            let last = pos;
            pos = (pos + 1) & self.mask;
            loop {
                let Some(curr) = &self.key[pos] else {
                    self.key[last] = None;
                    self.value[last] = None;
                    return;
                };
                let slot = mix(curr) as u32 as usize & self.mask;
                let stays = if last <= pos {
                    last >= slot || slot > pos
                } else {
                    last >= slot && slot > pos
                };
                if stays {
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            self.key[last] = self.key[pos].take();
            self.value[last] = self.value[pos].take();
        }
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key: Vec<Option<String>> = vec![None; new_n + 1];
        let mut new_value: Vec<Option<V>> =
            std::iter::repeat_with(|| None).take(new_n + 1).collect();
        let mut i = self.n;
        for _ in 0..self.size {
            loop {
                i -= 1;
                if self.key[i].is_some() {
                    break;
                }
            }
            let k = self.key[i].take().expect("occupied");
            let mut pos = mix(&k) as u32 as usize & mask;
            while new_key[pos].is_some() {
                pos = (pos + 1) & mask;
            }
            new_key[pos] = Some(k);
            new_value[pos] = self.value[i].take();
        }
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n, self.f);
        self.key = new_key;
        self.value = new_value;
    }

    /// Entries in iterator / `forEach` order (table backwards).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        (0..self.n)
            .rev()
            .filter_map(|p| Some((self.key[p].as_deref()?, self.value[p].as_ref()?)))
    }

    /// Entries in `stream()` order (table forwards).
    pub fn iter_stream(&self) -> impl Iterator<Item = (&str, &V)> {
        (0..self.n).filter_map(|p| Some((self.key[p].as_deref()?, self.value[p].as_ref()?)))
    }

    /// Mutable access to every value (order irrelevant).
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.value.iter_mut().filter_map(Option::as_mut)
    }
}

impl<V> std::ops::Index<&str> for ObjectOpenHashMap<V> {
    type Output = V;

    fn index(&self, k: &str) -> &V {
        self.get(k).expect("key present")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replays `fixtures/object_hash_order.txt` (generator `ObjectHashOrderGolden.java`).
    #[test]
    fn orders_match_fastutil() {
        let golden = include_str!("../../tests/fixtures/object_hash_order.txt");
        let mut lines = golden.lines();
        while let Some(header) = lines.next() {
            let mut map: ObjectOpenHashMap<()> = if header.starts_with('H') {
                ObjectOpenHashMap::with_expected(16, 0.5)
            } else {
                ObjectOpenHashMap::new()
            };
            for op in header[2..].split(' ') {
                match op.strip_prefix('+') {
                    Some(k) => {
                        map.insert(k.to_owned(), ());
                    }
                    None => {
                        map.remove(&op[1..]);
                    }
                }
            }
            let expect = |line: &str, prefix: &str| -> Vec<String> {
                line.strip_prefix(prefix)
                    .expect("prefix")
                    .split(' ')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            };
            let iterator = expect(lines.next().unwrap(), "I ");
            let for_each = expect(lines.next().unwrap(), "F ");
            let stream = expect(lines.next().unwrap(), "S ");
            lines.next();
            assert_eq!(
                map.iter().map(|(k, _)| k.to_owned()).collect::<Vec<_>>(),
                iterator
            );
            assert_eq!(iterator, for_each);
            assert_eq!(
                map.iter_stream()
                    .map(|(k, _)| k.to_owned())
                    .collect::<Vec<_>>(),
                stream
            );
        }
    }
}
