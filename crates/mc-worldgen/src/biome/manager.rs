//! `BiomeResolver` and `BiomeManager` (the fiddled 4x4x4 biome lookup).

use std::collections::HashSet;

use mc_core::math::java::floor_mod_i64;
use mc_core::math::mth;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::quart_pos;
use mc_world::palette::BiomeId;

/// `BiomeResolver`: biome at quart (4x4x4 block) coordinates.
pub trait BiomeResolver {
    /// `getNoiseBiome(quartX, quartY, quartZ)`.
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId;

    /// `getBiomesWithin(x, y, z, radius)`.
    fn get_biomes_within(&self, x: i32, y: i32, z: i32, radius: i32) -> HashSet<BiomeId> {
        let x0 = quart_pos::from_block(x.wrapping_sub(radius));
        let y0 = quart_pos::from_block(y.wrapping_sub(radius));
        let z0 = quart_pos::from_block(z.wrapping_sub(radius));
        let x1 = quart_pos::from_block(x.wrapping_add(radius));
        let y1 = quart_pos::from_block(y.wrapping_add(radius));
        let z1 = quart_pos::from_block(z.wrapping_add(radius));
        let mut biomes = HashSet::new();
        for noise_z in z0..=z1 {
            for noise_x in x0..=x1 {
                for noise_y in y0..=y1 {
                    biomes.insert(self.get_noise_biome(noise_x, noise_y, noise_z));
                }
            }
        }
        biomes
    }
}

/// Closures are resolvers (`BiomeResolver` is a `@FunctionalInterface`). A `Box<dyn BiomeResolver>` is used as
/// `&*resolver`.
impl<F: Fn(i32, i32, i32) -> BiomeId> BiomeResolver for F {
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self(quart_x, quart_y, quart_z)
    }
}

/// `LinearCongruentialGenerator.next`.
fn lcg_next(seed: i64, increment: i64) -> i64 {
    let seed = seed.wrapping_mul(
        seed.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407),
    );
    seed.wrapping_add(increment)
}

/// `BiomeManager`: resolves block-resolution biomes by choosing, with a seed-dependent jitter, the closest
/// of the 8 surrounding quart cells.
#[derive(Clone, Copy)]
pub struct BiomeManager<'a> {
    noise_biome_source: &'a dyn BiomeResolver,
    biome_zoom_seed: i64,
}

impl<'a> BiomeManager<'a> {
    /// `BiomeManager.CHUNK_CENTER_QUART`.
    pub const CHUNK_CENTER_QUART: i32 = quart_pos::from_block(8);

    /// `new BiomeManager(noiseBiomeSource, seed)`; `seed` is already obfuscated ([`Self::obfuscate_seed`]).
    pub fn new(noise_biome_source: &'a dyn BiomeResolver, seed: i64) -> BiomeManager<'a> {
        BiomeManager {
            noise_biome_source,
            biome_zoom_seed: seed,
        }
    }

    /// `obfuscateSeed(seed)`: `Hashing.sha256().hashLong(seed).asLong()`.
    pub fn obfuscate_seed(seed: i64) -> i64 {
        let digest = sha256_of_long(seed);
        i64::from_le_bytes(digest[..8].try_into().expect("digest has 32 bytes"))
    }

    /// `withDifferentSource(biomeSource)`.
    pub fn with_different_source<'b>(
        &self,
        biome_source: &'b dyn BiomeResolver,
    ) -> BiomeManager<'b> {
        BiomeManager {
            noise_biome_source: biome_source,
            biome_zoom_seed: self.biome_zoom_seed,
        }
    }

    /// `getBiome(BlockPos)`.
    pub fn get_biome(&self, pos: BlockPos) -> BiomeId {
        self.get_biome_at(pos.x(), pos.y(), pos.z())
    }

    /// `getBiome(x, y, z)`.
    pub fn get_biome_at(&self, x: i32, y: i32, z: i32) -> BiomeId {
        let abs_x = x.wrapping_sub(2);
        let abs_y = y.wrapping_sub(2);
        let abs_z = z.wrapping_sub(2);
        let parent_x = abs_x >> 2;
        let parent_y = abs_y >> 2;
        let parent_z = abs_z >> 2;
        let fract_x = f64::from(abs_x & 3) / 4.0;
        let fract_y = f64::from(abs_y & 3) / 4.0;
        let fract_z = f64::from(abs_z & 3) / 4.0;
        let mut min_i = 0;
        let mut min_fiddled_distance = f64::INFINITY;
        for i in 0..8 {
            let x_even = (i & 4) == 0;
            let y_even = (i & 2) == 0;
            let z_even = (i & 1) == 0;
            let corner_x = if x_even {
                parent_x
            } else {
                parent_x.wrapping_add(1)
            };
            let corner_y = if y_even {
                parent_y
            } else {
                parent_y.wrapping_add(1)
            };
            let corner_z = if z_even {
                parent_z
            } else {
                parent_z.wrapping_add(1)
            };
            let distance_x = if x_even { fract_x } else { fract_x - 1.0 };
            let distance_y = if y_even { fract_y } else { fract_y - 1.0 };
            let distance_z = if z_even { fract_z } else { fract_z - 1.0 };
            let next = get_fiddled_distance(
                self.biome_zoom_seed,
                corner_x,
                corner_y,
                corner_z,
                distance_x,
                distance_y,
                distance_z,
            );
            if min_fiddled_distance > next {
                min_i = i;
                min_fiddled_distance = next;
            }
        }
        let biome_x = if (min_i & 4) == 0 {
            parent_x
        } else {
            parent_x.wrapping_add(1)
        };
        let biome_y = if (min_i & 2) == 0 {
            parent_y
        } else {
            parent_y.wrapping_add(1)
        };
        let biome_z = if (min_i & 1) == 0 {
            parent_z
        } else {
            parent_z.wrapping_add(1)
        };
        self.noise_biome_source
            .get_noise_biome(biome_x, biome_y, biome_z)
    }

    /// `getNoiseBiomeAtPosition(x, y, z)`.
    pub fn get_noise_biome_at_position_f64(&self, x: f64, y: f64, z: f64) -> BiomeId {
        let quart_x = quart_pos::from_block(mth::floor_f64(x));
        let quart_y = quart_pos::from_block(mth::floor_f64(y));
        let quart_z = quart_pos::from_block(mth::floor_f64(z));
        self.get_noise_biome_at_quart(quart_x, quart_y, quart_z)
    }

    /// `getNoiseBiomeAtPosition(BlockPos)`.
    pub fn get_noise_biome_at_position(&self, pos: BlockPos) -> BiomeId {
        self.get_noise_biome_at_quart(
            quart_pos::from_block(pos.x()),
            quart_pos::from_block(pos.y()),
            quart_pos::from_block(pos.z()),
        )
    }

    /// `getNoiseBiomeAtQuart(quartX, quartY, quartZ)`.
    pub fn get_noise_biome_at_quart(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.noise_biome_source
            .get_noise_biome(quart_x, quart_y, quart_z)
    }
}

fn get_fiddled_distance(
    seed: i64,
    x_random: i32,
    y_random: i32,
    z_random: i32,
    distance_x: f64,
    distance_y: f64,
    distance_z: f64,
) -> f64 {
    let (xr, yr, zr) = (
        i64::from(x_random),
        i64::from(y_random),
        i64::from(z_random),
    );
    let mut rval = lcg_next(seed, xr);
    rval = lcg_next(rval, yr);
    rval = lcg_next(rval, zr);
    rval = lcg_next(rval, xr);
    rval = lcg_next(rval, yr);
    rval = lcg_next(rval, zr);
    let fiddle_x = get_fiddle(rval);
    rval = lcg_next(rval, seed);
    let fiddle_y = get_fiddle(rval);
    rval = lcg_next(rval, seed);
    let fiddle_z = get_fiddle(rval);
    mth::square_f64(distance_z + fiddle_z)
        + mth::square_f64(distance_y + fiddle_y)
        + mth::square_f64(distance_x + fiddle_x)
}

fn get_fiddle(rval: i64) -> f64 {
    let uniform = floor_mod_i64(rval >> 24, 1024) as f64 / 1024.0;
    (uniform - 0.5) * 0.9
}

/// SHA-256 of the 8 little-endian bytes of `value` (Guava `Hashing.sha256().hashLong`).
fn sha256_of_long(value: i64) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    // One padded block: the 8 message bytes, 0x80, zeros, the bit length (64) as a big-endian u64.
    let mut block = [0u8; 64];
    block[..8].copy_from_slice(&value.to_le_bytes());
    block[8] = 0x80;
    block[56..].copy_from_slice(&64u64.to_be_bytes());
    let mut w = [0u32; 64];
    for (i, chunk) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (state, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *state = state.wrapping_add(value);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}
