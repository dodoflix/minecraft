//! fastutil `LongOpenHashSet` (load factor 0.75, default size 16): same table layout, so iteration order
//! (`spliterator()`: the null key first, then the array from index 0 up) matches Java's for any add/remove history.
//! Vanilla's `PlayerChunkSender.pendingChunks` streams it, which makes iteration order observable (chunk send order of
//! chunks at equal distance).

const LONG_PHI: i64 = 0x9E37_79B9_7F4A_7C15u64 as i64;
const DEFAULT_INITIAL_SIZE: usize = 16;

/// `HashCommon.mix(long)` truncated to `int`.
fn mix(x: i64) -> i32 {
    let mut h = x.wrapping_mul(LONG_PHI);
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32
}

/// `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    let n = ((expected as f64) / 0.75).ceil() as u64;
    (if n <= 1 { 1 } else { (n - 1).next_power_of_two() }).max(2) as usize
}

/// `HashCommon.maxFill(n, 0.75f)`.
fn max_fill(n: usize) -> usize {
    (((n as f64) * 0.75).ceil() as usize).min(n - 1)
}

pub struct LongOpenHashSet {
    key: Vec<i64>,
    mask: usize,
    contains_null_key: bool,
    n: usize,
    min_n: usize,
    max_fill: usize,
    size: usize,
}

impl Default for LongOpenHashSet {
    fn default() -> Self {
        let n = array_size(DEFAULT_INITIAL_SIZE);
        Self { key: vec![0; n + 1], mask: n - 1, contains_null_key: false, n, min_n: n, max_fill: max_fill(n), size: 0 }
    }
}

impl LongOpenHashSet {
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
        let mut pos = mix(k) as usize & self.mask;
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

    pub fn contains(&self, k: i64) -> bool {
        self.find(k).is_ok()
    }

    /// `add`; returns whether the set changed.
    pub fn add(&mut self, k: i64) -> bool {
        match self.find(k) {
            Ok(_) => false,
            Err(pos) => {
                if pos == self.n {
                    self.contains_null_key = true;
                }
                self.key[pos] = k;
                let old = self.size;
                self.size += 1;
                if old >= self.max_fill {
                    self.rehash(array_size(self.size + 1));
                }
                true
            }
        }
    }

    /// `remove`; returns whether the set changed.
    pub fn remove(&mut self, k: i64) -> bool {
        match self.find(k) {
            Err(_) => false,
            Ok(pos) => {
                if k == 0 {
                    self.contains_null_key = false;
                } else {
                    self.shift_keys(pos);
                }
                self.size -= 1;
                if self.n > self.min_n && self.size < self.max_fill / 4 && self.n > DEFAULT_INITIAL_SIZE {
                    self.rehash(self.n / 2);
                }
                true
            }
        }
    }

    fn shift_keys(&mut self, mut pos: usize) {
        loop {
            let last = pos;
            pos = (pos + 1) & self.mask;
            let curr;
            loop {
                let c = self.key[pos];
                if c == 0 {
                    self.key[last] = 0;
                    return;
                }
                let slot = mix(c) as usize & self.mask;
                let stays = if last <= pos { last >= slot || slot > pos } else { last >= slot && slot > pos };
                if stays {
                    curr = c;
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            self.key[last] = curr;
        }
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key = vec![0i64; new_n + 1];
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
            let mut pos = mix(self.key[i]) as usize & mask;
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

    /// Stream order of `LongOpenHashSet.spliterator()`.
    pub fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        self.contains_null_key.then_some(0i64).into_iter().chain(self.key[..self.n].iter().copied().filter(|&k| k != 0))
    }
}
