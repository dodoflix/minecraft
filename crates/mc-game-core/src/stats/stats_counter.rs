//! `net.minecraft.stats.StatsCounter`.
use super::hash_order::{DEFAULT_TABLE, object_open_hash_order};
use super::stat::{Stat, build_name};
use indexmap::IndexMap;
use mc_core::math::java::java_string_hash;

pub struct StatsCounter {
    /// `stats` in insertion order (the order of the `Object2IntOpenHashMap`'s `put`s; `fast_order` derives its iteration order).
    pub(super) stats: IndexMap<Stat, i32>,
    /// Table size the `Object2IntOpenHashMap` started with (`putAll` presizes it).
    pub(super) table: usize,
}

impl Default for StatsCounter {
    fn default() -> Self {
        Self {
            stats: IndexMap::new(),
            table: DEFAULT_TABLE,
        }
    }
}

impl StatsCounter {
    /// `increment(player, stat, count)`: saturates at `Integer.MAX_VALUE`.
    pub fn increment(&mut self, stat: Stat, count: i32) {
        let result =
            (i64::from(self.get_value(&stat)) + i64::from(count)).min(i64::from(i32::MAX)) as i32;
        self.set_value(stat, result);
    }

    pub fn set_value(&mut self, stat: Stat, count: i32) {
        self.stats.insert(stat, count);
    }

    pub fn get_value(&self, stat: &Stat) -> i32 {
        self.stats.get(stat).copied().unwrap_or(0)
    }

    /// Entries in the iteration order of vanilla's `Object2IntOpenHashMap` (keys hash as `Stat.hashCode()`).
    pub fn iter_fast_order(&self) -> Vec<(Stat, i32)> {
        let entries: Vec<(&Stat, &i32)> = self.stats.iter().collect();
        let hashes: Vec<i32> = entries
            .iter()
            .map(|(s, _)| java_string_hash(&build_name(s)))
            .collect();
        object_open_hash_order(&hashes, self.table)
            .into_iter()
            .map(|i| (*entries[i].0, *entries[i].1))
            .collect()
    }
}
