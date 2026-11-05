//! Port of `RandomSource` and the levelgen random implementations.

mod bit;
mod legacy;
mod marsaglia;
mod random_source;
mod support;
mod worldgen_random;
mod xoroshiro;

pub use bit::{BitRandomSource, DOUBLE_MULTIPLIER, FLOAT_MULTIPLIER};
pub use legacy::{
    LegacyPositionalRandomFactory, LegacyRandomSource, SingleThreadedRandomSource,
    ThreadSafeLegacyRandomSource,
};
pub use marsaglia::MarsagliaPolarGaussian;
pub use random_source::{
    create, create_new, create_thread_local_instance, create_thread_local_instance_new,
    create_thread_safe, PositionalRandomFactory, RandomSource, GAUSSIAN_SPREAD_FACTOR,
};
pub use support::{
    generate_unique_seed, mix_stafford13, reseed_uniquifier, seed_from_hash_of, upgrade_seed_to_128bit,
    upgrade_seed_to_128bit_unmixed, Seed128bit, GOLDEN_RATIO_64, SILVER_RATIO_64,
};
pub use worldgen_random::{Algorithm, WorldgenRandom};
pub use xoroshiro::{Xoroshiro128PlusPlus, XoroshiroPositionalRandomFactory, XoroshiroRandomSource};

