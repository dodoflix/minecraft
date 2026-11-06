//! `BlockPatternBuilder`.

use super::block_in_world::BlockInWorld;
use super::block_pattern::{BlockPattern, Predicate};
use crate::world::level::LevelReader;
use std::collections::HashMap;
use std::sync::Arc;

pub struct BlockPatternBuilder {
    pattern: Vec<Vec<Vec<char>>>,
    lookup: HashMap<char, Predicate>,
    height: usize,
    width: usize,
    unknown_characters: Vec<char>,
}

impl BlockPatternBuilder {
    /// `start()`: `' '` matches anything.
    pub fn start() -> Self {
        let mut lookup: HashMap<char, Predicate> = HashMap::new();
        lookup.insert(
            ' ',
            Arc::new(|_: &mut BlockInWorld, _: &dyn LevelReader| true),
        );
        BlockPatternBuilder {
            pattern: Vec::new(),
            lookup,
            height: 0,
            width: 0,
            unknown_characters: Vec::new(),
        }
    }

    /// `aisle(String...)`.
    pub fn aisle(mut self, aisle: &[&str]) -> Self {
        assert!(
            !aisle.is_empty() && !aisle[0].is_empty(),
            "Empty pattern for aisle"
        );
        if self.pattern.is_empty() {
            self.height = aisle.len();
            self.width = aisle[0].chars().count();
        }
        assert!(
            aisle.len() == self.height,
            "Expected aisle with height of {}, but was given one with a height of {})",
            self.height,
            aisle.len()
        );
        let mut rows = Vec::with_capacity(aisle.len());
        for row in aisle {
            let chars: Vec<char> = row.chars().collect();
            assert!(
                chars.len() == self.width,
                "Not all rows in the given aisle are the correct width (expected {}, found one with {})",
                self.width,
                chars.len()
            );
            for &c in &chars {
                if !self.lookup.contains_key(&c) && !self.unknown_characters.contains(&c) {
                    self.unknown_characters.push(c);
                }
            }
            rows.push(chars);
        }
        self.pattern.push(rows);
        self
    }

    /// `where(char, Predicate<BlockInWorld>)`.
    pub fn where_(
        mut self,
        character: char,
        predicate: impl Fn(&mut BlockInWorld, &dyn LevelReader) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.lookup.insert(character, Arc::new(predicate));
        self.unknown_characters.retain(|&c| c != character);
        self
    }

    /// `build()`.
    pub fn build(self) -> BlockPattern {
        assert!(
            self.unknown_characters.is_empty(),
            "Predicates for character(s) {:?} are missing",
            self.unknown_characters
        );
        let pattern = self
            .pattern
            .iter()
            .map(|aisle| {
                aisle
                    .iter()
                    .map(|row| row.iter().map(|c| self.lookup[c].clone()).collect())
                    .collect()
            })
            .collect();
        BlockPattern::new(pattern)
    }
}
