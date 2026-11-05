//! `ShufflingList<U>`: weighted entries, `shuffle()` orders them by `-pow(random, 1 / weight)` (one `nextFloat` per
//! entry, in list order, from the list's own random source).

use mc_core::random::{LegacyRandomSource, RandomSource, create_new};

struct WeightedEntry<U> {
    data: U,
    weight: i32,
    rand_weight: f64,
}

pub struct ShufflingList<U> {
    entries: Vec<WeightedEntry<U>>,
    random: LegacyRandomSource,
}

impl<U> Default for ShufflingList<U> {
    fn default() -> Self {
        Self::new()
    }
}

impl<U> ShufflingList<U> {
    /// `new ShufflingList<>()` (its random is `RandomSource.create()`).
    pub fn new() -> Self {
        Self { entries: Vec::new(), random: create_new() }
    }

    /// Same with an explicit random source (tests).
    pub fn with_random(random: LegacyRandomSource) -> Self {
        Self { entries: Vec::new(), random }
    }

    /// `add(data, weight)`.
    pub fn add(&mut self, data: U, weight: i32) -> &mut Self {
        self.entries.push(WeightedEntry { data, weight, rand_weight: 0.0 });
        self
    }

    /// `shuffle()`: `randWeight = -Math.pow(random.nextFloat(), 1.0F / weight)`, then a stable sort by `Double.compare`.
    pub fn shuffle(&mut self) -> &mut Self {
        for e in &mut self.entries {
            let random = self.random.next_float();
            e.rand_weight = -f64::from(random).powf(f64::from(1.0f32 / e.weight as f32));
        }
        self.entries.sort_by(|a, b| a.rand_weight.total_cmp(&b.rand_weight));
        self
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `iterator()` / `stream()`: the data in the current order.
    pub fn iter(&self) -> impl Iterator<Item = &U> {
        self.entries.iter().map(|e| &e.data)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut U> {
        self.entries.iter_mut().map(|e| &mut e.data)
    }

    /// The weight of each entry in the current order.
    pub fn weights(&self) -> Vec<i32> {
        self.entries.iter().map(|e| e.weight).collect()
    }
}
