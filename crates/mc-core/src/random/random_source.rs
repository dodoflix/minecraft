use super::{LegacyRandomSource, SingleThreadedRandomSource, ThreadSafeLegacyRandomSource};
use crate::ident::Identifier;
use crate::pos::BlockPos;

pub const GAUSSIAN_SPREAD_FACTOR: f64 = 2.297;

/// `RandomSource`. Panics (vanilla: `IllegalArgumentException`) on non-positive bounds.
pub trait RandomSource: Send {
    fn fork(&mut self) -> Box<dyn RandomSource>;
    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory>;
    fn set_seed(&mut self, seed: i64);
    fn next_int(&mut self) -> i32;
    /// `nextInt(bound)`
    fn next_int_bounded(&mut self, bound: i32) -> i32;

    fn next_int_between_inclusive(&mut self, min: i32, max_inclusive: i32) -> i32 {
        self.next_int_bounded(max_inclusive.wrapping_sub(min).wrapping_add(1))
            .wrapping_add(min)
    }

    fn next_long(&mut self) -> i64;
    fn next_boolean(&mut self) -> bool;
    fn next_float(&mut self) -> f32;
    fn next_double(&mut self) -> f64;
    fn next_gaussian(&mut self) -> f64;

    fn triangle(&mut self, mean: f64, spread: f64) -> f64 {
        mean + spread * (self.next_double() - self.next_double())
    }

    fn triangle_f32(&mut self, mean: f32, spread: f32) -> f32 {
        mean + spread * (self.next_float() - self.next_float())
    }

    fn consume_count(&mut self, rounds: usize) {
        for _ in 0..rounds {
            self.next_int();
        }
    }

    /// `nextInt(origin, bound)`
    fn next_int_between(&mut self, origin: i32, bound: i32) -> i32 {
        assert!(origin < bound, "bound - origin is non positive");
        origin.wrapping_add(self.next_int_bounded(bound.wrapping_sub(origin)))
    }

    /// Emulates `source instanceof LegacyRandomSource` as used by `WorldgenRandom.next`:
    /// `Some(next(bits))` for `LegacyRandomSource` and `WorldgenRandom`, `None` otherwise.
    #[doc(hidden)]
    fn legacy_next(&mut self, _bits: i32) -> Option<i32> {
        None
    }
}

pub trait PositionalRandomFactory: Send + Sync {
    fn at(&self, x: i32, y: i32, z: i32) -> Box<dyn RandomSource>;
    fn from_hash_of(&self, name: &str) -> Box<dyn RandomSource>;
    fn from_seed(&self, seed: i64) -> Box<dyn RandomSource>;
    fn parity_config_string(&self, sb: &mut String);

    fn at_pos(&self, pos: BlockPos) -> Box<dyn RandomSource> {
        self.at(pos.x(), pos.y(), pos.z())
    }

    fn from_hash_of_identifier(&self, name: &Identifier) -> Box<dyn RandomSource> {
        self.from_hash_of(&name.to_string())
    }
}

pub fn create(seed: i64) -> LegacyRandomSource {
    LegacyRandomSource::new(seed)
}

pub fn create_new() -> LegacyRandomSource {
    create(super::generate_unique_seed())
}

/// Deprecated in vanilla.
pub fn create_thread_safe() -> ThreadSafeLegacyRandomSource {
    ThreadSafeLegacyRandomSource::new(super::generate_unique_seed())
}

/// `createThreadLocalInstance(seed)`
pub fn create_thread_local_instance(seed: i64) -> SingleThreadedRandomSource {
    SingleThreadedRandomSource::new(seed)
}

/// `createThreadLocalInstance()`: seeded from process entropy (vanilla: `ThreadLocalRandom.nextLong()`).
pub fn create_thread_local_instance_new() -> SingleThreadedRandomSource {
    use std::hash::{BuildHasher, Hasher};
    let seed = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish() as i64;
    SingleThreadedRandomSource::new(seed)
}
