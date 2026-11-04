//! `Stopwatches` (`data/minecraft/stopwatches.dat`) and `Stopwatch`.

use std::collections::{BTreeMap, HashMap};

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};

use super::{SavedData, SavedDataType};
use crate::util::codec::{self, DataResult, DecodeResult, map_decoded};
use crate::util::util::get_millis;

/// `net.minecraft.world.Stopwatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopwatch {
    pub creation_time: i64,
    pub accumulated_elapsed_time: i64,
}

impl Stopwatch {
    pub fn new(creation_time: i64) -> Self {
        Stopwatch { creation_time, accumulated_elapsed_time: 0 }
    }

    pub fn elapsed_milliseconds(&self, current_time: i64) -> i64 {
        self.accumulated_elapsed_time.wrapping_add(current_time.wrapping_sub(self.creation_time))
    }

    pub fn elapsed_seconds(&self, current_time: i64) -> f64 {
        self.elapsed_milliseconds(current_time) as f64 / 1000.0
    }
}

#[derive(Debug, Default)]
pub struct Stopwatches {
    dirty: bool,
    stopwatches: HashMap<Identifier, Stopwatch>,
}

impl SavedData for Stopwatches {
    /// Overridden in vanilla: a non-empty stopwatch set is always dirty (elapsed time changes without a mutator call).
    fn is_dirty(&self) -> bool {
        self.dirty || !self.stopwatches.is_empty()
    }
    fn set_dirty_to(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
}

impl Stopwatches {
    /// `Stopwatches.currentTime`.
    pub fn current_time() -> i64 {
        get_millis()
    }

    pub fn get(&self, id: &Identifier) -> Option<&Stopwatch> {
        self.stopwatches.get(id)
    }

    pub fn add(&mut self, id: Identifier, stopwatch: Stopwatch) -> bool {
        if let std::collections::hash_map::Entry::Vacant(e) = self.stopwatches.entry(id) {
            e.insert(stopwatch);
            self.set_dirty();
            true
        } else {
            false
        }
    }

    pub fn update(&mut self, id: &Identifier, update: impl FnOnce(Stopwatch) -> Stopwatch) -> bool {
        match self.stopwatches.get_mut(id) {
            Some(value) => {
                *value = update(*value);
                self.set_dirty();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: &Identifier) -> bool {
        let removed = self.stopwatches.remove(id).is_some();
        if removed {
            self.set_dirty();
        }
        removed
    }

    pub fn ids(&self) -> Vec<Identifier> {
        self.stopwatches.keys().cloned().collect()
    }

    fn unpack(stopwatches: Vec<(Identifier, i64)>, current_time: i64) -> Stopwatches {
        Stopwatches {
            dirty: false,
            stopwatches: stopwatches.into_iter().map(|(id, acc)| (id, Stopwatch { creation_time: current_time, accumulated_elapsed_time: acc })).collect(),
        }
    }

    /// `pack`: a `TreeMap` (sorted ids) of elapsed milliseconds.
    fn pack(&self, current_time: i64) -> BTreeMap<Identifier, i64> {
        self.stopwatches.iter().map(|(id, s)| (id.clone(), s.elapsed_milliseconds(current_time))).collect()
    }

    fn decode(tag: &Tag) -> DecodeResult<Stopwatches> {
        let m = codec::compound(tag)?;
        let map = codec::unbounded_map(codec::field(m, "stopwatches")?, Identifier::read, codec::long);
        map_decoded(map, |map| Stopwatches::unpack(map, Stopwatches::current_time()))
    }

    fn encode(&self) -> DataResult<Tag> {
        let mut inner = CompoundTag::new();
        for (id, elapsed) in self.pack(Stopwatches::current_time()) {
            inner.put(id.to_string(), Tag::Long(elapsed));
        }
        Ok(Tag::Compound(CompoundTag::new().with("stopwatches", inner)))
    }

    pub fn type_() -> SavedDataType<Stopwatches> {
        SavedDataType::new(Identifier::parse("minecraft:stopwatches").expect("static id"), Stopwatches::default, Stopwatches::decode, Stopwatches::encode)
    }
}
