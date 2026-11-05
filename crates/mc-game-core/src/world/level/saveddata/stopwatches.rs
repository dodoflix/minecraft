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

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    #[test]
    fn stopwatch_math_and_dirty_rules() {
        let s = Stopwatch { creation_time: 100, accumulated_elapsed_time: 50 };
        assert_eq!(s.elapsed_milliseconds(160), 110);
        assert_eq!(s.elapsed_seconds(1100), 1.05);
        let mut sw = Stopwatches::default();
        assert!(!sw.is_dirty());
        assert!(sw.add(id("a:b"), Stopwatch::new(0)));
        assert!(!sw.add(id("a:b"), Stopwatch::new(1)), "putIfAbsent");
        sw.set_dirty_to(false);
        assert!(sw.is_dirty(), "non-empty stopwatches are always dirty");
        assert!(sw.update(&id("a:b"), |s| Stopwatch { accumulated_elapsed_time: 5, ..s }));
        assert!(!sw.update(&id("x:y"), |s| s));
        assert_eq!(sw.get(&id("a:b")).unwrap().accumulated_elapsed_time, 5);
        assert!(sw.remove(&id("a:b")));
        sw.set_dirty_to(false);
        assert!(!sw.is_dirty());
    }

    #[test]
    fn codec_unpacks_relative_to_now() {
        let ty = Stopwatches::type_();
        let sw = (ty.decode)(&mc_nbt::snbt::parse("{stopwatches:{\"a:b\":1500L}}").unwrap()).map_err(|e| e.error).unwrap();
        let Tag::Compound(c) = (ty.encode)(&sw).unwrap() else { panic!() };
        let elapsed = c.get_compound("stopwatches").unwrap().get_long("a:b").unwrap();
        assert!((1500..2500).contains(&elapsed), "{elapsed}");
    }
}
