use super::{
    GOLDEN_RATIO_64, MarsagliaPolarGaussian, PositionalRandomFactory, RandomSource,
    SILVER_RATIO_64, Seed128bit, seed_from_hash_of, upgrade_seed_to_128bit,
};

const FLOAT_UNIT: f32 = 5.9604645E-8;
const DOUBLE_UNIT: f64 = 1.110223E-16_f32 as f64;

/// `Xoroshiro128PlusPlus` (the Codec, a 2-long stream, is `[seed_lo, seed_hi]` via `to_longs`/`from_longs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Xoroshiro128PlusPlus {
    seed_lo: i64,
    seed_hi: i64,
}

impl Xoroshiro128PlusPlus {
    pub fn new(seed_lo: i64, seed_hi: i64) -> Self {
        if seed_lo | seed_hi == 0 {
            Self {
                seed_lo: GOLDEN_RATIO_64,
                seed_hi: SILVER_RATIO_64,
            }
        } else {
            Self { seed_lo, seed_hi }
        }
    }

    pub fn from_seed(seed: Seed128bit) -> Self {
        Self::new(seed.seed_lo, seed.seed_hi)
    }

    pub fn from_longs([lo, hi]: [i64; 2]) -> Self {
        Self::new(lo, hi)
    }

    pub fn to_longs(self) -> [i64; 2] {
        [self.seed_lo, self.seed_hi]
    }

    pub fn next_long(&mut self) -> i64 {
        let s0 = self.seed_lo;
        let mut s1 = self.seed_hi;
        let result = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        s1 ^= s0;
        self.seed_lo = s0.rotate_left(49) ^ s1 ^ (s1 << 21);
        self.seed_hi = s1.rotate_left(28);
        result
    }
}

#[derive(Clone, Debug)]
pub struct XoroshiroRandomSource {
    random_number_generator: Xoroshiro128PlusPlus,
    gaussian: MarsagliaPolarGaussian,
}

impl XoroshiroRandomSource {
    pub fn new(seed: i64) -> Self {
        Self::from_generator(Xoroshiro128PlusPlus::from_seed(upgrade_seed_to_128bit(
            seed,
        )))
    }

    pub fn from_seed128(seed: Seed128bit) -> Self {
        Self::from_generator(Xoroshiro128PlusPlus::from_seed(seed))
    }

    pub fn from_seeds(seed_lo: i64, seed_hi: i64) -> Self {
        Self::from_generator(Xoroshiro128PlusPlus::new(seed_lo, seed_hi))
    }

    pub fn from_generator(random_number_generator: Xoroshiro128PlusPlus) -> Self {
        Self {
            random_number_generator,
            gaussian: MarsagliaPolarGaussian::new(),
        }
    }

    pub fn generator(&self) -> Xoroshiro128PlusPlus {
        self.random_number_generator
    }

    fn next_bits(&mut self, bits: i32) -> i64 {
        (self.random_number_generator.next_long() as u64 >> (64 - bits)) as i64
    }
}

impl RandomSource for XoroshiroRandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        let lo = self.random_number_generator.next_long();
        let hi = self.random_number_generator.next_long();
        Box::new(XoroshiroRandomSource::from_seeds(lo, hi))
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        let lo = self.random_number_generator.next_long();
        let hi = self.random_number_generator.next_long();
        Box::new(XoroshiroPositionalRandomFactory::new(lo, hi))
    }

    fn set_seed(&mut self, seed: i64) {
        self.random_number_generator =
            Xoroshiro128PlusPlus::from_seed(upgrade_seed_to_128bit(seed));
        self.gaussian.reset();
    }

    fn next_int(&mut self) -> i32 {
        self.random_number_generator.next_long() as i32
    }

    fn next_int_bounded(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Bound must be positive");
        let bound_l = bound as i64;
        let mut random_bits = self.next_int() as u32 as i64;
        let mut multiplied = random_bits * bound_l;
        let mut fractional = multiplied & 0xFFFF_FFFF;
        if fractional < bound_l {
            let unbiased_start = ((!bound).wrapping_add(1) as u32 % bound as u32) as i64;
            while fractional < unbiased_start {
                random_bits = self.next_int() as u32 as i64;
                multiplied = random_bits * bound_l;
                fractional = multiplied & 0xFFFF_FFFF;
            }
        }
        (multiplied >> 32) as i32
    }

    fn next_long(&mut self) -> i64 {
        self.random_number_generator.next_long()
    }

    fn next_boolean(&mut self) -> bool {
        self.random_number_generator.next_long() & 1 != 0
    }

    fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * FLOAT_UNIT
    }

    fn next_double(&mut self) -> f64 {
        self.next_bits(53) as f64 * DOUBLE_UNIT
    }

    fn next_gaussian(&mut self) -> f64 {
        let mut g = self.gaussian;
        let v = g.next_gaussian(self);
        self.gaussian = g;
        v
    }

    fn consume_count(&mut self, rounds: usize) {
        for _ in 0..rounds {
            self.random_number_generator.next_long();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct XoroshiroPositionalRandomFactory {
    seed_lo: i64,
    seed_hi: i64,
}

impl XoroshiroPositionalRandomFactory {
    pub const fn new(seed_lo: i64, seed_hi: i64) -> Self {
        Self { seed_lo, seed_hi }
    }
}

impl PositionalRandomFactory for XoroshiroPositionalRandomFactory {
    fn at(&self, x: i32, y: i32, z: i32) -> Box<dyn RandomSource> {
        Box::new(XoroshiroRandomSource::from_seeds(
            crate::math::mth::get_seed_xyz(x, y, z) ^ self.seed_lo,
            self.seed_hi,
        ))
    }

    fn from_hash_of(&self, name: &str) -> Box<dyn RandomSource> {
        Box::new(XoroshiroRandomSource::from_seed128(
            seed_from_hash_of(name).xor(self.seed_lo, self.seed_hi),
        ))
    }

    fn from_seed(&self, seed: i64) -> Box<dyn RandomSource> {
        Box::new(XoroshiroRandomSource::from_seeds(
            seed ^ self.seed_lo,
            seed ^ self.seed_hi,
        ))
    }

    fn parity_config_string(&self, sb: &mut String) {
        sb.push_str(&format!(
            "seedLo: {}, seedHi: {}",
            self.seed_lo, self.seed_hi
        ));
    }
}
