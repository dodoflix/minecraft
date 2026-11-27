//! `EndSpikeFeature.EndSpike` and `getSpikesForLevel`: where the ten obsidian pillars of the End stand (the feature that
//! places them is `end_spike_feature`; the dragon fight reads the same list to count the crystals on them).

use super::super::world_gen_level::WorldGenLevel;
use mc_core::math::{java, mth};
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::{RandomSource, SingleThreadedRandomSource};
use mc_worldgen::noise_settings::{DIMENSION_MAX_Y, DIMENSION_MIN_Y};

const NUMBER_OF_SPIKES: usize = 10;
const SPIKE_DISTANCE: f64 = 42.0;

/// `EndSpikeFeature.EndSpike`.
#[derive(Clone, Debug, PartialEq)]
pub struct EndSpike {
    center_x: i32,
    center_z: i32,
    radius: i32,
    height: i32,
    guarded: bool,
    top_bounding_box: AABB,
}

impl EndSpike {
    pub fn new(center_x: i32, center_z: i32, radius: i32, height: i32, guarded: bool) -> EndSpike {
        EndSpike {
            center_x,
            center_z,
            radius,
            height,
            guarded,
            top_bounding_box: AABB::new(
                center_x.wrapping_sub(radius) as f64,
                DIMENSION_MIN_Y as f64,
                center_z.wrapping_sub(radius) as f64,
                center_x.wrapping_add(radius) as f64,
                DIMENSION_MAX_Y as f64,
                center_z.wrapping_add(radius) as f64,
            ),
        }
    }

    pub fn is_center_within_chunk(&self, chunk_origin: BlockPos) -> bool {
        (chunk_origin.x() >> 4) == (self.center_x >> 4)
            && (chunk_origin.z() >> 4) == (self.center_z >> 4)
    }

    pub fn get_center_x(&self) -> i32 {
        self.center_x
    }

    pub fn get_center_z(&self) -> i32 {
        self.center_z
    }

    pub fn get_radius(&self) -> i32 {
        self.radius
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub fn is_guarded(&self) -> bool {
        self.guarded
    }

    pub fn get_top_bounding_box(&self) -> &AABB {
        &self.top_bounding_box
    }
}

/// `SpikeCacheLoader.load(seed)`.
fn load_spikes(seed: i64) -> Vec<EndSpike> {
    let mut sizes: Vec<i32> = (0..NUMBER_OF_SPIKES as i32).collect();
    let mut random = SingleThreadedRandomSource::new(seed);
    for i in (2..=sizes.len()).rev() {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        sizes.swap(i - 1, swap_to);
    }
    let mut result = Vec::with_capacity(NUMBER_OF_SPIKES);
    for (i, &size) in sizes.iter().enumerate() {
        let angle = 2.0 * (-std::f64::consts::PI + (std::f64::consts::PI / 10.0) * i as f64);
        let x = mth::floor_f64(SPIKE_DISTANCE * java::cos(angle));
        let z = mth::floor_f64(SPIKE_DISTANCE * java::sin(angle));
        let radius = 2 + size / 3;
        let height = 76 + size * 3;
        let guarded = size == 1 || size == 2;
        result.push(EndSpike::new(x, z, radius, height, guarded));
    }
    result
}

/// `getSpikesForLevel(level)`: the cache key is the first long of a thread-local random seeded with the level seed (masked
/// to 16 bits); the spikes only depend on that key (the Guava cache is unobservable).
pub fn get_spikes_for_seed(level_seed: i64) -> Vec<EndSpike> {
    let mut random = SingleThreadedRandomSource::new(level_seed);
    let key = random.next_long() & 65535;
    load_spikes(key)
}

/// `getSpikesForLevel(level)`.
pub fn get_spikes_for_level(level: &dyn WorldGenLevel) -> Vec<EndSpike> {
    get_spikes_for_seed(level.get_seed())
}
