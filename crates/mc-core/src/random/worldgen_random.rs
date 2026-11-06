use super::bit::bit_random_source_methods;
use super::{
    BitRandomSource, LegacyRandomSource, MarsagliaPolarGaussian, PositionalRandomFactory,
    RandomSource, SingleThreadedRandomSource, XoroshiroRandomSource, create_thread_local_instance,
};

/// `WorldgenRandom` (extends `LegacyRandomSource`: all derived methods go through `next(bits)`,
/// `next_gaussian` uses its own cached gaussian which `set_seed` does NOT reset, as in vanilla).
pub struct WorldgenRandom {
    random_source: Box<dyn RandomSource>,
    count: i32,
    gaussian: MarsagliaPolarGaussian,
}

impl WorldgenRandom {
    pub fn new(random_source: Box<dyn RandomSource>) -> Self {
        Self {
            random_source,
            count: 0,
            gaussian: MarsagliaPolarGaussian::new(),
        }
    }

    pub fn count(&self) -> i32 {
        self.count
    }

    pub fn set_decoration_seed(&mut self, seed: i64, chunk_x: i32, chunk_z: i32) -> i64 {
        self.set_seed(seed);
        let x_scale = self.next_long() | 1;
        let z_scale = self.next_long() | 1;
        let result = (chunk_x as i64)
            .wrapping_mul(x_scale)
            .wrapping_add((chunk_z as i64).wrapping_mul(z_scale))
            ^ seed;
        self.set_seed(result);
        result
    }

    pub fn set_feature_seed(&mut self, seed: i64, index: i32, step: i32) {
        let result = seed
            .wrapping_add(index as i64)
            .wrapping_add(10000i32.wrapping_mul(step) as i64);
        self.set_seed(result);
    }

    pub fn set_large_feature_seed(&mut self, seed: i64, chunk_x: i32, chunk_z: i32) {
        self.set_seed(seed);
        let x_scale = self.next_long();
        let z_scale = self.next_long();
        let result =
            (chunk_x as i64).wrapping_mul(x_scale) ^ (chunk_z as i64).wrapping_mul(z_scale) ^ seed;
        self.set_seed(result);
    }

    pub fn set_large_feature_with_salt(&mut self, seed: i64, x: i32, z: i32, blend: i32) {
        let result = (x as i64)
            .wrapping_mul(341873128712)
            .wrapping_add((z as i64).wrapping_mul(132897987541))
            .wrapping_add(seed)
            .wrapping_add(blend as i64);
        self.set_seed(result);
    }

    pub fn seed_slime_chunk(x: i32, z: i32, seed: i64, salt: i64) -> SingleThreadedRandomSource {
        let seed = seed
            .wrapping_add(x.wrapping_mul(x).wrapping_mul(4987142) as i64)
            .wrapping_add(x.wrapping_mul(5947611) as i64)
            .wrapping_add((z.wrapping_mul(z) as i64).wrapping_mul(4392871))
            .wrapping_add(z.wrapping_mul(389711) as i64)
            ^ salt;
        create_thread_local_instance(seed)
    }
}

impl BitRandomSource for WorldgenRandom {
    fn next(&mut self, bits: i32) -> i32 {
        self.count = self.count.wrapping_add(1);
        match self.random_source.legacy_next(bits) {
            Some(v) => v,
            None => (self.random_source.next_long() as u64 >> (64 - bits)) as i32,
        }
    }
}

impl RandomSource for WorldgenRandom {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        self.random_source.fork()
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        self.random_source.fork_positional()
    }

    fn set_seed(&mut self, seed: i64) {
        self.random_source.set_seed(seed);
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

/// `WorldgenRandom.Algorithm`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Algorithm {
    Legacy,
    Xoroshiro,
}

impl Algorithm {
    pub fn new_instance(self, seed: i64) -> Box<dyn RandomSource> {
        match self {
            Algorithm::Legacy => Box::new(LegacyRandomSource::new(seed)),
            Algorithm::Xoroshiro => Box::new(XoroshiroRandomSource::new(seed)),
        }
    }
}
