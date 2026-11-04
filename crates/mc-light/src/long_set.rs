//! fastutil 8.5 `LongOpenHashSet` (open addressing, linear probing, backwards iteration): the iteration order is
//! observable because the light engine processes queued block checks in set order.

/// `HashCommon.mix(long)`.
fn mix(x: i64) -> usize {
    let mut h = x.wrapping_mul(-7046029254386353131); // LONG_PHI = 0x9E3779B97F4A7C15
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32 as u32 as usize
}

fn next_power_of_two(x: u64) -> u64 {
    if x == 0 { 1 } else { (x - 1).next_power_of_two().max(1) }
}

/// `HashCommon.arraySize(expected, f)`.
fn array_size(expected: usize, f: f32) -> usize {
    let s = next_power_of_two(((expected as f32 / f) as f64).ceil() as u64).max(2);
    s as usize
}

/// `HashCommon.maxFill(n, f)`.
fn max_fill(n: usize, f: f32) -> usize {
    (((n as f32 * f) as f64).ceil() as usize).min(n - 1)
}

#[derive(Clone, Debug)]
pub struct LongOpenHashSet {
    /// `key[n]` is the slot of the `0` key in fastutil; here that key lives in `contains_null`.
    key: Vec<i64>,
    mask: usize,
    contains_null: bool,
    n: usize,
    max_fill: usize,
    size: usize,
    f: f32,
}

impl Default for LongOpenHashSet {
    fn default() -> Self {
        Self::with_capacity(16, 0.75)
    }
}

impl LongOpenHashSet {
    /// `new LongOpenHashSet(expected, f)`.
    pub fn with_capacity(expected: usize, f: f32) -> Self {
        let n = array_size(expected, f);
        LongOpenHashSet { key: vec![0; n], mask: n - 1, contains_null: false, n, max_fill: max_fill(n, f), size: 0, f }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn add(&mut self, k: i64) -> bool {
        if k == 0 {
            if self.contains_null {
                return false;
            }
            self.contains_null = true;
        } else {
            let mut pos = mix(k) & self.mask;
            loop {
                let curr = self.key[pos];
                if curr == 0 {
                    break;
                }
                if curr == k {
                    return false;
                }
                pos = (pos + 1) & self.mask;
            }
            self.key[pos] = k;
        }
        let old = self.size;
        self.size += 1;
        if old >= self.max_fill {
            self.rehash(array_size(self.size + 1, self.f));
        }
        true
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key = vec![0i64; new_n];
        let mut i = self.n;
        let real = if self.contains_null { self.size - 1 } else { self.size };
        for _ in 0..real {
            loop {
                i -= 1;
                if self.key[i] != 0 {
                    break;
                }
            }
            let mut pos = mix(self.key[i]) & mask;
            while new_key[pos] != 0 {
                pos = (pos + 1) & mask;
            }
            new_key[pos] = self.key[i];
        }
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n, self.f);
        self.key = new_key;
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        if self.size == 0 {
            return;
        }
        self.size = 0;
        self.contains_null = false;
        self.key.fill(0);
    }

    /// `trim(n)`.
    pub fn trim(&mut self, n: usize) {
        let l = next_power_of_two(((n as f32 / self.f) as f64).ceil() as u64) as usize;
        if l >= self.n || self.size > max_fill(l, self.f) {
            return;
        }
        self.rehash(l);
    }

    /// Iteration order of `iterator()`: the `0` key first, then slots from the top down.
    pub fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        self.contains_null.then_some(0).into_iter().chain(self.key.iter().rev().copied().filter(|&k| k != 0))
    }
}
