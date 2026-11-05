//! `EndSpikeFeature` (`end_spike`): the obsidian pillars of the End with their end crystals.

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature, set_block};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_block_pos, parse_bool, parse_int, parse_list};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::BlockGetter;
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource, SingleThreadedRandomSource};
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_nbt::{CompoundTag, ListTag};
use mc_worldgen::noise_settings::{DIMENSION_MAX_Y, DIMENSION_MIN_Y};
use serde_json::Value;
use std::sync::Arc;

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

    /// `EndSpike.CODEC`.
    pub fn from_json(value: &Value) -> LoadResult<EndSpike> {
        let fields = Fields::new(value)?;
        Ok(EndSpike::new(
            fields.optional("centerX", parse_int)?.unwrap_or(0),
            fields.optional("centerZ", parse_int)?.unwrap_or(0),
            fields.optional("radius", parse_int)?.unwrap_or(0),
            fields.optional("height", parse_int)?.unwrap_or(0),
            fields.optional("guarded", parse_bool)?.unwrap_or(false),
        ))
    }

    pub fn is_center_within_chunk(&self, chunk_origin: BlockPos) -> bool {
        (chunk_origin.x() >> 4) == (self.center_x >> 4) && (chunk_origin.z() >> 4) == (self.center_z >> 4)
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
        let x = mth::floor_f64(SPIKE_DISTANCE * angle.cos());
        let z = mth::floor_f64(SPIKE_DISTANCE * angle.sin());
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

pub struct EndSpikeFeature {
    pub spikes: Vec<EndSpike>,
    pub crystal_invulnerable: bool,
    pub crystal_beam_target: Option<BlockPos>,
}

impl EndSpikeFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(EndSpikeFeature {
            spikes: fields.required("spikes", |v| parse_list(v, EndSpike::from_json))?,
            crystal_invulnerable: fields.optional("crystal_invulnerable", parse_bool)?.unwrap_or(false),
            crystal_beam_target: fields.optional("crystal_beam_target", parse_block_pos)?,
        }))
    }

    fn place_spike(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, spike: &EndSpike) {
        let radius = spike.get_radius();
        let min_y = level.get_min_y();
        for pos in BlockPos::between_closed(
            &BlockPos::new(spike.get_center_x() - radius, min_y, spike.get_center_z() - radius),
            &BlockPos::new(spike.get_center_x() + radius, spike.get_height() + 10, spike.get_center_z() + radius),
        ) {
            if pos.dist_to_low_corner_sqr(spike.get_center_x() as f64, pos.y() as f64, spike.get_center_z() as f64) <= (radius * radius + 1) as f64
                && pos.y() < spike.get_height()
            {
                set_block(level, pos, Block::OBSIDIAN.default_state());
            } else if pos.y() > 65 {
                set_block(level, pos, Block::AIR.default_state());
            }
        }

        if spike.is_guarded() {
            for dx in -2i32..=2 {
                for dz in -2i32..=2 {
                    for dy in 0..=3 {
                        let is_x_side = dx.abs() == 2;
                        let is_z_side = dz.abs() == 2;
                        let top = dy == 3;
                        if is_x_side || is_z_side || top {
                            let x_edge = dx == -2 || dx == 2 || top;
                            let z_edge = dz == -2 || dz == 2 || top;
                            let state = Block::IRON_BARS
                                .default_state()
                                .set(props::NORTH, x_edge && dz != -2)
                                .set(props::SOUTH, x_edge && dz != 2)
                                .set(props::WEST, z_edge && dx != -2)
                                .set(props::EAST, z_edge && dx != 2);
                            set_block(level, BlockPos::new(spike.get_center_x() + dx, spike.get_height() + dy, spike.get_center_z() + dz), state);
                        }
                    }
                }
            }
        }

        // `EntityTypes.END_CRYSTAL.create(..)`, `setBeamTarget`, `setPermanentlyInvulnerable`, `snapTo(..)`, `addFreshEntity`.
        let x = spike.get_center_x() as f64 + 0.5;
        let y = (spike.get_height() + 1) as f64;
        let z = spike.get_center_z() as f64 + 0.5;
        let yaw = random.next_float() * 360.0;
        level.add_fresh_entity_nbt(end_crystal_tag(x, y, z, yaw, self.crystal_beam_target, self.crystal_invulnerable));
        let crystal_pos = BlockPos::new(mth::floor_f64(x), mth::floor_f64(y), mth::floor_f64(z));
        set_block(level, crystal_pos.below(), Block::BEDROCK.default_state());
        let fire = mc_game_core::world::level::block::impls::base_fire_block::get_state(&*level, crystal_pos);
        set_block(level, crystal_pos, fire);
    }
}

/// `EndCrystal.save`: the entity NBT as `ProtoChunk.addEntity` stores it (the UUID is random in vanilla too).
fn end_crystal_tag(x: f64, y: f64, z: f64, yaw: f32, beam_target: Option<BlockPos>, invulnerable: bool) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", "minecraft:end_crystal");
    let mut pos = ListTag::new();
    for v in [x, y, z] {
        pos.push(v);
    }
    tag.put("Pos", pos);
    let mut motion = ListTag::new();
    for _ in 0..3 {
        motion.push(0.0f64);
    }
    tag.put("Motion", motion);
    let mut rotation = ListTag::new();
    rotation.push(yaw);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_double("fall_distance", 0.0);
    tag.put_short("Fire", 0);
    tag.put_short("Air", 300);
    tag.put_boolean("OnGround", false);
    tag.put_boolean("Invulnerable", invulnerable);
    tag.put_int("PortalCooldown", 0);
    let mut random = LegacyRandomSource::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64));
    let uuid = mth::create_insecure_uuid(&mut random);
    tag.put_int_array("UUID", mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec());
    if let Some(target) = beam_target {
        tag.put_int_array("beam_target", vec![target.x(), target.y(), target.z()]);
    }
    tag.put_boolean("ShowBottom", true);
    tag
}

impl Feature for EndSpikeFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let computed;
        let spikes = if self.spikes.is_empty() {
            computed = get_spikes_for_level(level);
            &computed
        } else {
            &self.spikes
        };
        for spike in spikes {
            if spike.is_center_within_chunk(origin) {
                self.place_spike(level, random, spike);
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static END_SPIKE: TypeEntry<dyn Feature> = TypeEntry { name: "end_spike", decode: EndSpikeFeature::decode };

