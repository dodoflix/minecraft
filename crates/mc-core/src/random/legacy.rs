use super::bit::bit_random_source_methods;
use super::{
    BitRandomSource, MarsagliaPolarGaussian, PositionalRandomFactory,
    RandomSource,
};
use std::sync::atomic::{AtomicI64, Ordering};

const MODULUS_BITS: i32 = 48;
const MODULUS_MASK: i64 = 281474976710655;
const MULTIPLIER: i64 = 25214903917;
const INCREMENT: i64 = 11;

fn scramble(seed: i64) -> i64 {
    (seed ^ MULTIPLIER) & MODULUS_MASK
}

fn step(seed: i64) -> i64 {
    seed.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT) & MODULUS_MASK
}

fn out(new_seed: i64, bits: i32) -> i32 {
    (new_seed as u64 >> (MODULUS_BITS - bits)) as i32
}

/// `LegacyRandomSource` (java.util.Random LCG). Vanilla's `AtomicLong` is only a threading
/// detector; `&mut self` already enforces exclusivity here.
#[derive(Clone, Debug)]
pub struct LegacyRandomSource {
    seed: i64,
    gaussian: MarsagliaPolarGaussian,
}

impl LegacyRandomSource {
    pub fn new(seed: i64) -> Self {
        Self { seed: scramble(seed), gaussian: MarsagliaPolarGaussian::new() }
    }
}

impl BitRandomSource for LegacyRandomSource {
    fn next(&mut self, bits: i32) -> i32 {
        self.seed = step(self.seed);
        out(self.seed, bits)
    }
}

impl RandomSource for LegacyRandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(self.next_long()))
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        Box::new(LegacyPositionalRandomFactory::new(self.next_long()))
    }

    fn set_seed(&mut self, seed: i64) {
        self.seed = scramble(seed);
        self.gaussian.reset();
    }

    bit_random_source_methods!();

    fn next_gaussian(&mut self) -> f64 {
        let mut g = self.gaussian;
        let v = g.next_gaussian(self);
        self.gaussian = g;
        v
    }

    fn legacy_next(&mut self, bits: i32) -> Option<i32> {
        Some(self.next(bits))
    }
}

/// `ThreadSafeLegacyRandomSource`: same sequence, seed advanced with a CAS loop so `next` works on `&self`.
#[derive(Debug)]
pub struct ThreadSafeLegacyRandomSource {
    seed: AtomicI64,
    gaussian: MarsagliaPolarGaussian,
}

impl ThreadSafeLegacyRandomSource {
    pub fn new(seed: i64) -> Self {
        Self { seed: AtomicI64::new(scramble(seed)), gaussian: MarsagliaPolarGaussian::new() }
    }

    /// Thread-safe `next(bits)`.
    pub fn next_shared(&self, bits: i32) -> i32 {
        let mut old = self.seed.load(Ordering::SeqCst);
        loop {
            let next = step(old);
            match self.seed.compare_exchange(old, next, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return out(next, bits),
                Err(actual) => old = actual,
            }
        }
    }
}

impl BitRandomSource for ThreadSafeLegacyRandomSource {
    fn next(&mut self, bits: i32) -> i32 {
        self.next_shared(bits)
    }
}

impl RandomSource for ThreadSafeLegacyRandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        Box::new(ThreadSafeLegacyRandomSource::new(self.next_long()))
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        Box::new(LegacyPositionalRandomFactory::new(self.next_long()))
    }

    fn set_seed(&mut self, seed: i64) {
        self.seed.store(scramble(seed), Ordering::SeqCst);
    }

    bit_random_source_methods!();

    fn next_gaussian(&mut self) -> f64 {
        let mut g = self.gaussian;
        let v = g.next_gaussian(self);
        self.gaussian = g;
        v
    }
}

/// `SingleThreadedRandomSource`
#[derive(Clone, Debug)]
pub struct SingleThreadedRandomSource {
    seed: i64,
    gaussian: MarsagliaPolarGaussian,
}

impl SingleThreadedRandomSource {
    pub fn new(seed: i64) -> Self {
        Self { seed: scramble(seed), gaussian: MarsagliaPolarGaussian::new() }
    }
}

impl BitRandomSource for SingleThreadedRandomSource {
    fn next(&mut self, bits: i32) -> i32 {
        self.seed = step(self.seed);
        out(self.seed, bits)
    }
}

impl RandomSource for SingleThreadedRandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        Box::new(SingleThreadedRandomSource::new(self.next_long()))
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        Box::new(LegacyPositionalRandomFactory::new(self.next_long()))
    }

    fn set_seed(&mut self, seed: i64) {
        self.seed = scramble(seed);
        self.gaussian.reset();
    }

    bit_random_source_methods!();

    fn next_gaussian(&mut self) -> f64 {
        let mut g = self.gaussian;
        let v = g.next_gaussian(self);
        self.gaussian = g;
        v
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LegacyPositionalRandomFactory {
    seed: i64,
}

impl LegacyPositionalRandomFactory {
    pub const fn new(seed: i64) -> Self {
        Self { seed }
    }
}

impl PositionalRandomFactory for LegacyPositionalRandomFactory {
    fn at(&self, x: i32, y: i32, z: i32) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(crate::math::mth::get_seed_xyz(x, y, z) ^ self.seed))
    }

    fn from_hash_of(&self, name: &str) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(crate::math::java::java_string_hash(name) as i64 ^ self.seed))
    }

    fn from_seed(&self, seed: i64) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(seed))
    }

    fn parity_config_string(&self, sb: &mut String) {
        sb.push_str(&format!("LegacyPositionalRandomFactory{{{}}}", self.seed));
    }
}
