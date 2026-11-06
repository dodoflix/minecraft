//! `CaveWorldCarver` (`minecraft:cave`; the overworld caves and the nether caves are two configurations).

use serde_json::Value;

use mc_core::math::mth;
use mc_core::pos::ChunkPos;
use mc_core::random::{RandomSource, create_thread_local_instance};
use mc_world::carving_mask::CarverOutput;

use super::{CarveSkipChecker, WorldCarver, can_reach, carve_ellipsoid, parse_float_range};
use crate::densityfunction::codec::{Fields, LoadResult, parse_bool};
use crate::provider::{FloatProvider, HeightProvider, IntProvider};
use crate::world_generation_context::WorldGenerationContext;

/// `(float)(Math.PI * 2)`.
const TWO_PI: f32 = (std::f64::consts::PI * 2.0) as f32;
/// `(float)(Math.PI / 2)`.
const HALF_PI: f32 = (std::f64::consts::PI / 2.0) as f32;
/// `(float)Math.PI`.
const PI: f32 = std::f64::consts::PI as f32;

#[derive(Clone, Debug, PartialEq)]
pub struct CaveWorldCarver {
    pub probability: f32,
    pub y: HeightProvider,
    pub count: IntProvider,
    pub thickness: FloatProvider,
    pub weird_thickness_bias: bool,
    pub room_vertical_radius_multiplier: FloatProvider,
    pub horizontal_radius_multiplier: FloatProvider,
    pub vertical_radius_multiplier: FloatProvider,
    pub start_vertical_radius_multiplier: FloatProvider,
    pub floor_level: FloatProvider,
}

impl CaveWorldCarver {
    /// `CaveWorldCarver.MAP_CODEC`.
    pub(crate) fn from_fields(f: &Fields<'_>) -> LoadResult<CaveWorldCarver> {
        Ok(CaveWorldCarver {
            probability: f.required("probability", |v| parse_float_range(v, 0.0, 1.0))?,
            y: f.required("y", HeightProvider::from_json)?,
            count: f.required("count", IntProvider::non_negative_from_json)?,
            thickness: f.required("thickness", |v| FloatProvider::from_json_min(v, 0.0))?,
            weird_thickness_bias: f
                .optional("weird_thickness_bias", parse_bool)?
                .unwrap_or(false),
            room_vertical_radius_multiplier: f
                .required("room_vertical_radius_multiplier", FloatProvider::from_json)?,
            horizontal_radius_multiplier: f
                .required("horizontal_radius_multiplier", FloatProvider::from_json)?,
            vertical_radius_multiplier: f
                .required("vertical_radius_multiplier", FloatProvider::from_json)?,
            start_vertical_radius_multiplier: f
                .optional("start_vertical_radius_multiplier", FloatProvider::from_json)?
                .unwrap_or(FloatProvider::constant(1.0)),
            floor_level: f.required("floor_level", |v| {
                FloatProvider::from_json_range(v, -1.0, 1.0)
            })?,
        })
    }

    pub fn from_json(value: &Value) -> LoadResult<CaveWorldCarver> {
        CaveWorldCarver::from_fields(&Fields::new(value)?)
    }

    fn get_thickness(&self, random: &mut dyn RandomSource) -> f32 {
        let mut thickness = self.thickness.sample(random);
        if self.weird_thickness_bias && random.next_int_bounded(10) == 0 {
            // Java: `thickness *= random.nextFloat() * random.nextFloat() * 3.0F + 1.0F`
            let a = random.next_float();
            let b = random.next_float();
            thickness *= a * b * 3.0f32 + 1.0f32;
        }
        thickness
    }

    fn create_room(
        &self,
        chunk_pos: ChunkPos,
        x: f64,
        y: f64,
        z: f64,
        thickness: f32,
        y_scale: f64,
        output: &mut dyn CarverOutput,
        skip_checker: &CarveSkipChecker<'_>,
    ) {
        let horizontal_radius = 1.5 + f64::from(mth::sin(f64::from(HALF_PI)) * thickness);
        let vertical_radius = horizontal_radius * y_scale;
        carve_ellipsoid(
            chunk_pos,
            x + 1.0,
            y,
            z,
            horizontal_radius,
            vertical_radius,
            output,
            skip_checker,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla `createTunnel` parameter list"
    )]
    fn create_tunnel(
        &self,
        chunk_pos: ChunkPos,
        tunnel_seed: i64,
        mut x: f64,
        mut y: f64,
        mut z: f64,
        horizontal_radius_multiplier: f64,
        vertical_radius_multiplier: f64,
        thickness: f32,
        mut horizontal_rotation: f32,
        mut vertical_rotation: f32,
        step: i32,
        dist: i32,
        y_scale: f64,
        output: &mut dyn CarverOutput,
        skip_checker: &CarveSkipChecker<'_>,
    ) {
        let mut random = create_thread_local_instance(tunnel_seed);
        let split_point = random.next_int_bounded(dist / 2) + dist / 4;
        let steep = random.next_int_bounded(6) == 0;
        let mut y_rota = 0.0f32;
        let mut x_rota = 0.0f32;

        for current_step in step..dist {
            let horizontal_radius = 1.5
                + f64::from(
                    mth::sin(f64::from(PI * current_step as f32 / dist as f32)) * thickness,
                );
            let vertical_radius = horizontal_radius * y_scale;
            let cos_x = mth::cos(f64::from(vertical_rotation));
            x += f64::from(mth::cos(f64::from(horizontal_rotation)) * cos_x);
            y += f64::from(mth::sin(f64::from(vertical_rotation)));
            z += f64::from(mth::sin(f64::from(horizontal_rotation)) * cos_x);
            vertical_rotation *= if steep { 0.92f32 } else { 0.7f32 };
            vertical_rotation += x_rota * 0.1f32;
            horizontal_rotation += y_rota * 0.1f32;
            x_rota *= 0.9f32;
            y_rota *= 0.75f32;
            x_rota += (random.next_float() - random.next_float()) * random.next_float() * 2.0f32;
            y_rota += (random.next_float() - random.next_float()) * random.next_float() * 4.0f32;
            if current_step == split_point && thickness > 1.0f32 {
                let seed_a = random.next_long();
                let thickness_a = random.next_float() * 0.5f32 + 0.5f32;
                self.create_tunnel(
                    chunk_pos,
                    seed_a,
                    x,
                    y,
                    z,
                    horizontal_radius_multiplier,
                    vertical_radius_multiplier,
                    thickness_a,
                    horizontal_rotation - HALF_PI,
                    vertical_rotation / 3.0f32,
                    current_step,
                    dist,
                    1.0,
                    output,
                    skip_checker,
                );
                let seed_b = random.next_long();
                let thickness_b = random.next_float() * 0.5f32 + 0.5f32;
                self.create_tunnel(
                    chunk_pos,
                    seed_b,
                    x,
                    y,
                    z,
                    horizontal_radius_multiplier,
                    vertical_radius_multiplier,
                    thickness_b,
                    horizontal_rotation + HALF_PI,
                    vertical_rotation / 3.0f32,
                    current_step,
                    dist,
                    1.0,
                    output,
                    skip_checker,
                );
                return;
            }

            if random.next_int_bounded(4) != 0 {
                if !can_reach(chunk_pos, x, z, current_step, dist, thickness) {
                    return;
                }
                carve_ellipsoid(
                    chunk_pos,
                    x,
                    y,
                    z,
                    horizontal_radius * horizontal_radius_multiplier,
                    vertical_radius * vertical_radius_multiplier,
                    output,
                    skip_checker,
                );
            }
        }
    }
}

/// `CaveWorldCarver.shouldSkip`.
fn should_skip(xd: f64, yd: f64, zd: f64, floor_level: f64) -> bool {
    if yd <= floor_level {
        true
    } else {
        xd * xd + yd * yd + zd * zd >= 1.0
    }
}

impl WorldCarver for CaveWorldCarver {
    fn is_start_chunk(&self, random: &mut dyn RandomSource) -> bool {
        random.next_float() <= self.probability
    }

    fn carve(
        &self,
        context: &WorldGenerationContext,
        random: &mut dyn RandomSource,
        chunk_pos: ChunkPos,
        source_chunk_pos: ChunkPos,
        output: &mut dyn CarverOutput,
    ) -> bool {
        // `SectionPos.sectionToBlockCoord(getRange() * 2 - 1)`
        let max_distance = (self.get_range() * 2 - 1) << 4;
        let cave_count = self.count.sample(random);

        for _ in 0..cave_count {
            let x = f64::from(source_chunk_pos.get_block_x(random.next_int_bounded(16)));
            let y = f64::from(self.y.sample(random, context));
            let z = f64::from(source_chunk_pos.get_block_z(random.next_int_bounded(16)));
            let horizontal_radius_multiplier =
                f64::from(self.horizontal_radius_multiplier.sample(random));
            let vertical_radius_multiplier =
                f64::from(self.vertical_radius_multiplier.sample(random));
            let start_vertical_radius_multiplier =
                f64::from(self.start_vertical_radius_multiplier.sample(random));
            let floor_level = f64::from(self.floor_level.sample(random));
            let skip_checker = move |xd: f64, yd: f64, zd: f64, _world_y: i32| {
                should_skip(xd, yd, zd, floor_level)
            };
            let mut tunnels = 1;
            if random.next_int_bounded(4) == 0 {
                let y_scale = f64::from(self.room_vertical_radius_multiplier.sample(random));
                let thickness = 1.0f32 + random.next_float() * 6.0f32;
                self.create_room(
                    chunk_pos,
                    x,
                    y,
                    z,
                    thickness,
                    y_scale,
                    output,
                    &skip_checker,
                );
                tunnels += random.next_int_bounded(4);
            }

            for _ in 0..tunnels {
                let horizontal_rotation = random.next_float() * TWO_PI;
                let vertical_rotation = (random.next_float() - 0.5f32) / 4.0f32;
                let thickness = self.get_thickness(random);
                let distance = max_distance - random.next_int_bounded(max_distance / 4);
                let tunnel_seed = random.next_long();
                self.create_tunnel(
                    chunk_pos,
                    tunnel_seed,
                    x,
                    y,
                    z,
                    horizontal_radius_multiplier,
                    vertical_radius_multiplier,
                    thickness,
                    horizontal_rotation,
                    vertical_rotation,
                    0,
                    distance,
                    start_vertical_radius_multiplier,
                    output,
                    &skip_checker,
                );
            }
        }
        true
    }
}
