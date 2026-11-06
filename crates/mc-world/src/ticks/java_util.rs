//! Ports of the two JDK/fastutil containers whose *iteration order* is observable in vanilla's tick scheduling:
//! `java.util.PriorityQueue` (binary heap, array-order iteration, `Iterator.remove`) and fastutil 8.5
//! `Long2LongOpenHashMap` (open addressing with linear probing, backwards `fastIterator` with removal).

use std::cmp::Ordering;
use std::collections::VecDeque;

/// `java.util.PriorityQueue` with a comparator. Elements carry a private identity (`uid`) because the JDK
/// iterator's `remove()` bookkeeping (`forgetMeNot`, `removeEq`) works on object identity.
#[derive(Clone, Debug)]
pub struct JavaPriorityQueue<E> {
    items: Vec<(u64, E)>,
    next_uid: u64,
    cmp: fn(&E, &E) -> Ordering,
}

impl<E> JavaPriorityQueue<E> {
    pub fn new(cmp: fn(&E, &E) -> Ordering) -> Self {
        Self {
            items: Vec::new(),
            next_uid: 0,
            cmp,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn peek(&self) -> Option<&E> {
        self.items.first().map(|(_, e)| e)
    }

    /// Heap-array order, like `iterator()` / `toArray()` / `stream()`.
    pub fn iter(&self) -> impl Iterator<Item = &E> {
        self.items.iter().map(|(_, e)| e)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn add(&mut self, e: E) {
        let uid = self.next_uid;
        self.next_uid += 1;
        self.items.push((uid, e));
        let last = self.items.len() - 1;
        self.sift_up(last);
    }

    pub fn poll(&mut self) -> Option<E> {
        if self.items.is_empty() {
            return None;
        }
        let result = self.items.swap_remove(0);
        // swap_remove moved the last element to index 0, which is exactly `siftDown(0, x)` input
        if !self.items.is_empty() {
            self.sift_down(0);
        }
        Some(result.1)
    }

    /// `siftUp(k, x)` for the element currently stored at `k`; returns its final index.
    fn sift_up(&mut self, mut k: usize) -> usize {
        while k > 0 {
            let parent = (k - 1) >> 1;
            if (self.cmp)(&self.items[k].1, &self.items[parent].1) != Ordering::Less {
                break;
            }
            self.items.swap(k, parent);
            k = parent;
        }
        k
    }

    /// `siftDown(k, x)` for the element currently stored at `k`; returns its final index.
    fn sift_down(&mut self, mut k: usize) -> usize {
        let n = self.items.len();
        let half = n >> 1;
        while k < half {
            let mut child = (k << 1) + 1;
            let right = child + 1;
            if right < n
                && (self.cmp)(&self.items[child].1, &self.items[right].1) == Ordering::Greater
            {
                child = right;
            }
            if (self.cmp)(&self.items[k].1, &self.items[child].1) != Ordering::Greater {
                break;
            }
            self.items.swap(k, child);
            k = child;
        }
        k
    }

    /// `removeAt(i)`: the removed element and, if the displaced last element ended up *before* `i`, its uid.
    fn remove_at(&mut self, i: usize) -> (E, Option<u64>) {
        let s = self.items.len() - 1;
        if s == i {
            return (self.items.pop().expect("non-empty").1, None);
        }
        let removed = self.items.swap_remove(i); // last element now sits at i
        let moved_uid = self.items[i].0;
        let at = self.sift_down(i);
        if at == i {
            let at = self.sift_up(i);
            if at != i {
                return (removed.1, Some(moved_uid));
            }
        }
        (removed.1, None)
    }

    /// `iterator()` + `Iterator.remove()` for every element matching `test` (with JDK quirks: an element moved
    /// to an already-visited index is revisited through `forgetMeNot`). `on_removed` receives each removed element.
    pub fn remove_if(&mut self, mut test: impl FnMut(&E) -> bool, mut on_removed: impl FnMut(E)) {
        let mut cursor = 0usize;
        let mut forget_me_not: VecDeque<u64> = VecDeque::new();
        loop {
            // hasNext()/next()
            // `lastRet` is Some for a heap-array element, None for a `forgetMeNot` one (removed with `removeEq`)
            let (idx, last_ret);
            if cursor < self.items.len() {
                last_ret = Some(cursor);
                idx = cursor;
                cursor += 1;
            } else if let Some(u) = forget_me_not.pop_front() {
                last_ret = None;
                let Some(i) = self.items.iter().position(|(x, _)| *x == u) else {
                    continue;
                };
                idx = i;
            } else {
                break;
            }
            if !test(&self.items[idx].1) {
                continue;
            }
            // remove()
            let (removed, moved) = self.remove_at(idx);
            on_removed(removed);
            if last_ret.is_some() {
                match moved {
                    None => cursor -= 1,
                    Some(m) => forget_me_not.push_back(m),
                }
            }
        }
    }
}

const LONG_PHI: i64 = 0x9E37_79B9_7F4A_7C15u64 as i64;

/// `HashCommon.mix(long)` truncated to `int`.
fn mix(x: i64) -> i32 {
    let mut h = x.wrapping_mul(LONG_PHI);
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32
}

fn next_power_of_two(x: u64) -> u64 {
    if x == 0 {
        1
    } else {
        (x - 1).next_power_of_two().max(1)
    }
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

/// fastutil `Long2LongOpenHashMap` (load factor 0.75, default size 16). Only what `LevelTicks` uses.
pub struct Long2LongOpenHashMap {
    key: Vec<i64>,
    value: Vec<i64>,
    mask: usize,
    contains_null_key: bool,
    n: usize,
    min_n: usize,
    max_fill: usize,
    size: usize,
}

impl Default for Long2LongOpenHashMap {
    fn default() -> Self {
        let n = array_size(DEFAULT_INITIAL_SIZE);
        Self {
            key: vec![0; n + 1],
            value: vec![0; n + 1],
            mask: n - 1,
            contains_null_key: false,
            n,
            min_n: n,
            max_fill: max_fill(n),
            size: 0,
        }
    }
}

impl Long2LongOpenHashMap {
    fn find(&self, k: i64) -> Result<usize, usize> {
        if k == 0 {
            return if self.contains_null_key {
                Ok(self.n)
            } else {
                Err(self.n)
            };
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

    pub fn put(&mut self, k: i64, v: i64) {
        match self.find(k) {
            Ok(pos) => self.value[pos] = v,
            Err(pos) => {
                if pos == self.n {
                    self.contains_null_key = true;
                }
                self.key[pos] = k;
                self.value[pos] = v;
                let old = self.size;
                self.size += 1;
                if old >= self.max_fill {
                    self.rehash(array_size(self.size + 1));
                }
            }
        }
    }

    pub fn remove(&mut self, k: i64) {
        match self.find(k) {
            Err(_) => {}
            Ok(pos) if pos == self.n && k == 0 => {
                self.contains_null_key = false;
                self.size -= 1;
                self.maybe_shrink();
            }
            Ok(pos) => {
                self.size -= 1;
                self.shift_keys(pos, None);
                self.maybe_shrink();
            }
        }
    }

    fn maybe_shrink(&mut self) {
        if self.n > self.min_n && self.size < self.max_fill / 4 && self.n > DEFAULT_INITIAL_SIZE {
            self.rehash(self.n / 2);
        }
    }

    /// `shiftKeys`; with `wrapped` set this is the iterator's variant that records entries moved across the
    /// array end so the iterator can still visit them.
    fn shift_keys(&mut self, mut pos: usize, mut wrapped: Option<&mut Vec<i64>>) {
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
                let stays = if last <= pos {
                    last >= slot || slot > pos
                } else {
                    last >= slot && slot > pos
                };
                if stays {
                    curr = c;
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            if pos < last
                && let Some(w) = wrapped.as_deref_mut()
            {
                w.push(self.key[pos]);
            }
            self.key[last] = curr;
            self.value[last] = self.value[pos];
        }
    }

    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut new_key = vec![0i64; new_n + 1];
        let mut new_value = vec![0i64; new_n + 1];
        let mut i = self.n;
        let mut j = if self.contains_null_key {
            self.size - 1
        } else {
            self.size
        };
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
            new_value[pos] = self.value[i];
        }
        new_value[new_n] = self.value[self.n];
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n);
        self.key = new_key;
        self.value = new_value;
    }

    /// `Long2LongMaps.fastIterator(map)` loop: visits every entry in fastutil's order, `f(key, &mut value)` returns
    /// `true` to `Iterator.remove()` the entry just visited.
    pub fn fast_iterate(&mut self, mut f: impl FnMut(i64, &mut i64) -> bool) {
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
                        let mut p = mix(k) as usize & self.mask;
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
            let mut value = self.value[index];
            let remove = f(key, &mut value);
            self.value[index] = value;
            if remove {
                if last_is_null {
                    self.contains_null_key = false;
                    self.size -= 1;
                } else if !last_is_wrapped {
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
