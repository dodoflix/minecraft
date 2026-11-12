//! Port of fastutil 8.5 `Object2ObjectOpenHashMap` (open addressing, linear probing, load factor 0.75, iteration from the
//! last slot down). `AttributeInstance.modifiersByOperation` is one of these per operation (keyed by `Identifier`) and
//! `calculateValue` sums/multiplies in its iteration order, which is observable in the last bits of the `double` result;
//! `AttributeMap.attributes` is one keyed by attribute holder. Only the operations those use are ported (`put`, `remove`,
//! `values`, `clear`).

use mc_core::ident::Identifier;
use mc_core::math::java::java_string_hash;

const INT_PHI: i32 = 0x9E37_79B9u32 as i32;
const DEFAULT_INITIAL_SIZE: usize = 16;

/// `Identifier.hashCode()`.
pub fn identifier_hash(id: &Identifier) -> i32 {
    java_string_hash(id.namespace())
        .wrapping_mul(31)
        .wrapping_add(java_string_hash(id.path()))
}

/// The `hashCode()` of a map key.
pub trait JavaHash {
    fn java_hash(&self) -> i32;
}

impl JavaHash for Identifier {
    fn java_hash(&self) -> i32 {
        identifier_hash(self)
    }
}

/// `HashCommon.mix(int)`.
fn mix(x: i32) -> i32 {
    let h = x.wrapping_mul(INT_PHI);
    h ^ (((h as u32) >> 16) as i32)
}

fn slot<K: JavaHash>(k: &K, mask: usize) -> usize {
    mix(k.java_hash()) as usize & mask
}

fn max_fill(n: usize) -> usize {
    (((n as f64) * 0.75).ceil() as usize).min(n - 1)
}

/// `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    let wanted = ((expected as f64) / 0.75).ceil() as u64;
    (if wanted <= 1 {
        1
    } else {
        wanted.next_power_of_two()
    })
    .max(2) as usize
}

#[derive(Clone, Debug)]
pub struct ObjectOpenHashMap<K, V> {
    key: Vec<Option<K>>,
    value: Vec<Option<V>>,
    mask: usize,
    n: usize,
    max_fill: usize,
    size: usize,
    min_n: usize,
}

impl<K: JavaHash + Eq + Clone, V> Default for ObjectOpenHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: JavaHash + Eq + Clone, V> ObjectOpenHashMap<K, V> {
    /// `new Object2ObjectOpenHashMap<>()`.
    pub fn new() -> Self {
        let n = array_size(DEFAULT_INITIAL_SIZE);
        Self {
            key: vec![None; n],
            value: (0..n).map(|_| None).collect(),
            mask: n - 1,
            n,
            max_fill: max_fill(n),
            size: 0,
            min_n: n,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn find(&self, k: &K) -> Result<usize, usize> {
        let mut pos = slot(k, self.mask);
        loop {
            match &self.key[pos] {
                None => return Err(pos),
                Some(c) if c == k => return Ok(pos),
                _ => pos = (pos + 1) & self.mask,
            }
        }
    }

    /// `put`.
    pub fn insert(&mut self, k: K, v: V) {
        match self.find(&k) {
            Ok(pos) => self.value[pos] = Some(v),
            Err(pos) => {
                self.key[pos] = Some(k);
                self.value[pos] = Some(v);
                let old = self.size;
                self.size += 1;
                if old >= self.max_fill {
                    self.rehash(array_size(self.size + 1));
                }
            }
        }
    }

    /// `remove`.
    pub fn remove(&mut self, k: &K) {
        let Ok(pos) = self.find(k) else { return };
        self.size -= 1;
        self.shift_keys(pos);
        if self.n > self.min_n && self.size < self.max_fill / 4 && self.n > DEFAULT_INITIAL_SIZE {
            self.rehash(self.n / 2);
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    fn shift_keys(&mut self, mut pos: usize) {
        loop {
            let last = pos;
            pos = (pos + 1) & self.mask;
            loop {
                let Some(c) = &self.key[pos] else {
                    self.key[last] = None;
                    self.value[last] = None;
                    return;
                };
                let s = slot(c, self.mask);
                let stays = if last <= pos {
                    last >= s || s > pos
                } else {
                    last >= s && s > pos
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
        let mut new_key: Vec<Option<K>> = vec![None; new_n];
        let mut new_value: Vec<Option<V>> = (0..new_n).map(|_| None).collect();
        let mut i = self.n;
        let mut j = self.size;
        while j > 0 {
            j -= 1;
            loop {
                i -= 1;
                if self.key[i].is_some() {
                    break;
                }
            }
            let k = self.key[i].take().expect("occupied");
            let mut pos = slot(&k, mask);
            while new_key[pos].is_some() {
                pos = (pos + 1) & mask;
            }
            new_key[pos] = Some(k);
            new_value[pos] = self.value[i].take();
        }
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n);
        self.key = new_key;
        self.value = new_value;
    }

    /// `values()` in fastutil iteration order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        (0..self.n).rev().filter_map(|p| self.value[p].as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_remove_keeps_all_keys() {
        let mut m = ObjectOpenHashMap::new();
        for i in 0..40 {
            m.insert(Identifier::parse(&format!("a:k{i}")).unwrap(), i);
        }
        for i in 0..30 {
            m.remove(&Identifier::parse(&format!("a:k{i}")).unwrap());
        }
        let mut left: Vec<i32> = m.values().copied().collect();
        left.sort();
        assert_eq!(left, (30..40).collect::<Vec<_>>());
    }
}
