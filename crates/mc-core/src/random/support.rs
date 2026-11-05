use md5::{Digest, Md5};
use std::sync::atomic::{AtomicI64, Ordering};

pub const GOLDEN_RATIO_64: i64 = -7046029254386353131;
pub const SILVER_RATIO_64: i64 = 7640891576956012809;

static SEED_UNIQUIFIER: AtomicI64 = AtomicI64::new(8682522807148012);

pub fn mix_stafford13(mut z: i64) -> i64 {
    z = (z ^ (z as u64 >> 30) as i64).wrapping_mul(-4658895280553007687);
    z = (z ^ (z as u64 >> 27) as i64).wrapping_mul(-7723592293110705685);
    z ^ (z as u64 >> 31) as i64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seed128bit {
    pub seed_lo: i64,
    pub seed_hi: i64,
}

impl Seed128bit {
    pub const fn new(seed_lo: i64, seed_hi: i64) -> Self {
        Self { seed_lo, seed_hi }
    }

    pub fn xor(self, lo: i64, hi: i64) -> Self {
        Self::new(self.seed_lo ^ lo, self.seed_hi ^ hi)
    }

    pub fn xor_seed(self, other: Seed128bit) -> Self {
        self.xor(other.seed_lo, other.seed_hi)
    }

    pub fn mixed(self) -> Self {
        Self::new(mix_stafford13(self.seed_lo), mix_stafford13(self.seed_hi))
    }
}

pub fn upgrade_seed_to_128bit_unmixed(legacy_seed: i64) -> Seed128bit {
    let low_bits = legacy_seed ^ SILVER_RATIO_64;
    let high_bits = low_bits.wrapping_add(GOLDEN_RATIO_64);
    Seed128bit::new(low_bits, high_bits)
}

pub fn upgrade_seed_to_128bit(legacy_seed: i64) -> Seed128bit {
    upgrade_seed_to_128bit_unmixed(legacy_seed).mixed()
}

/// MD5 of the UTF-8 bytes, split into two big-endian longs.
pub fn seed_from_hash_of(input: &str) -> Seed128bit {
    let h = Md5::digest(input.as_bytes());
    let lo = i64::from_be_bytes(h[0..8].try_into().expect("md5 is 16 bytes"));
    let hi = i64::from_be_bytes(h[8..16].try_into().expect("md5 is 16 bytes"));
    Seed128bit::new(lo, hi)
}

thread_local! {
    /// Oracle mode (see [`reseed_uniquifier`]): the calling thread's own uniquifier, no clock.
    static ORACLE_UNIQUIFIER: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

/// Entity-oracle mode for the calling thread: `generate_unique_seed` becomes the reproducible sequence of the vanilla oracle's
/// `RandomSupport` shadow (`tests/oracle/shadow.patch`: a per-thread counter without `System.nanoTime()`), restarted at `value`.
/// Used by the scenario replay harness to give freshly created entities the same random seeds as the oracle.
pub fn reseed_uniquifier(value: i64) {
    ORACLE_UNIQUIFIER.with(|c| c.set(Some(value)));
}

pub fn generate_unique_seed() -> i64 {
    if let Some(current) = ORACLE_UNIQUIFIER.with(|c| c.get()) {
        let updated = current.wrapping_mul(1181783497276652981);
        ORACLE_UNIQUIFIER.with(|c| c.set(Some(updated)));
        return updated ^ 0x2545F4914F6CDD1D;
    }
    let prev = SEED_UNIQUIFIER
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| Some(c.wrapping_mul(1181783497276652981)))
        .expect("closure always returns Some");
    let updated = prev.wrapping_mul(1181783497276652981);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64);
    updated ^ nanos
}
