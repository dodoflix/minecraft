//! `net.minecraft.util.CrudeIncrementalIntIdentityHashBiMap`: an id <-> object map whose objects are compared by
//! identity (`Arc` pointer equality here) and whose ids are handed out in insertion order, reusing the lowest free id.

use crate::math::mth::murmur_hash3_mixer;
use std::sync::Arc;

#[derive(Debug)]
pub struct CrudeIncrementalIntIdentityHashBiMap<K> {
    keys: Vec<Option<Arc<K>>>,
    values: Vec<i32>,
    by_id: Vec<Option<Arc<K>>>,
    next_id: i32,
    size: i32,
}

/// `copy()`: the arrays are cloned, the keys stay shared.
impl<K> Clone for CrudeIncrementalIntIdentityHashBiMap<K> {
    fn clone(&self) -> Self {
        CrudeIncrementalIntIdentityHashBiMap {
            keys: self.keys.clone(),
            values: self.values.clone(),
            by_id: self.by_id.clone(),
            next_id: self.next_id,
            size: self.size,
        }
    }
}

impl<K> CrudeIncrementalIntIdentityHashBiMap<K> {
    fn with_capacity(capacity: usize) -> Self {
        CrudeIncrementalIntIdentityHashBiMap {
            keys: vec![None; capacity],
            values: vec![0; capacity],
            by_id: vec![None; capacity],
            next_id: 0,
            size: 0,
        }
    }

    /// `create(initialCapacity)`.
    pub fn create(initial_capacity: i32) -> Self {
        Self::with_capacity((initial_capacity as f32 / 0.8f32) as i32 as usize)
    }

    /// `getId(thing)`: `-1` when absent.
    pub fn get_id(&self, thing: &Arc<K>) -> i32 {
        self.index_of(thing, self.hash(thing))
            .map_or(-1, |index| self.values[index])
    }

    /// `byId(id)`.
    pub fn by_id(&self, id: i32) -> Option<&Arc<K>> {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.by_id.get(id))?
            .as_ref()
    }

    /// `contains(key)`.
    pub fn contains(&self, key: &Arc<K>) -> bool {
        self.get_id(key) != -1
    }

    /// `contains(id)`.
    pub fn contains_id(&self, id: i32) -> bool {
        self.by_id(id).is_some()
    }

    /// `add(key)`: the id the key was given.
    pub fn add(&mut self, key: Arc<K>) -> i32 {
        let value = self.next_id();
        self.add_mapping(key, value);
        value
    }

    fn next_id(&mut self) -> i32 {
        while (self.next_id as usize) < self.by_id.len()
            && self.by_id[self.next_id as usize].is_some()
        {
            self.next_id += 1;
        }
        self.next_id
    }

    fn grow(&mut self, new_size: usize) {
        let mut resized = Self::with_capacity(new_size);
        for (key, value) in self.keys.iter().zip(&self.values) {
            if let Some(key) = key {
                resized.add_mapping(key.clone(), *value);
            }
        }
        *self = resized;
    }

    /// `addMapping(key, id)`; panics (`ArrayIndexOutOfBoundsException`) when `id` does not fit the grown arrays.
    pub fn add_mapping(&mut self, key: Arc<K>, id: i32) {
        let min_size = id.max(self.size + 1);
        if min_size as f32 >= self.keys.len() as f32 * 0.8f32 {
            let mut new_size = self.keys.len() << 1;
            while (new_size as i64) < i64::from(id) {
                new_size <<= 1;
            }
            self.grow(new_size);
        }
        let index = self.find_empty(self.hash(&key));
        self.keys[index] = Some(key.clone());
        self.values[index] = id;
        self.by_id[id as usize] = Some(key);
        self.size += 1;
        if id == self.next_id {
            self.next_id += 1;
        }
    }

    /// `Mth.murmurHash3Mixer(System.identityHashCode(key))` reduced to a slot; the address plays the identity hash.
    fn hash(&self, key: &Arc<K>) -> usize {
        (murmur_hash3_mixer(Arc::as_ptr(key) as usize as i32) & i32::MAX) as usize % self.keys.len()
    }

    fn index_of(&self, key: &Arc<K>, start_from: usize) -> Option<usize> {
        for i in (start_from..self.keys.len()).chain(0..start_from) {
            match &self.keys[i] {
                Some(k) if Arc::ptr_eq(k, key) => return Some(i),
                Some(_) => {}
                None => return None,
            }
        }
        None
    }

    fn find_empty(&self, start_from: usize) -> usize {
        (start_from..self.keys.len())
            .chain(0..start_from)
            .find(|&i| self.keys[i].is_none())
            .expect("Overflowed :(")
    }

    /// `iterator()`: the keys in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<K>> {
        self.by_id.iter().flatten()
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.keys.fill(None);
        self.by_id.fill(None);
        self.next_id = 0;
        self.size = 0;
    }

    /// `size()`.
    pub fn size(&self) -> i32 {
        self.size
    }
}
