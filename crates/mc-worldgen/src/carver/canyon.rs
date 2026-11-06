//! `CanyonWorldCarver` (`minecraft:canyon`).

use serde_json::Value;

use mc_core::math::mth;
use mc_core::pos::ChunkPos;
use mc_core::random::{RandomSource, create_thread_local_instance};
use mc_world::carving_mask::CarverOutput;

use super::{WorldCarver, can_reach, carve_ellipsoid, parse_float_range};
use crate::densityfunction::codec::{Fields, LoadResult, parse_f32, parse_positive_int};
use crate::provider::{FloatProvider, HeightProvider};
use crate::world_generation_context::WorldGenerationContext;

/// `(float)(Math.PI * 2)`.
const TWO_PI: f32 = (std::f64::consts::PI * 2.0) as f32;
/// `(float)Math.PI`.
const PI: f32 = std::f64::consts::PI as f32;

/// `CanyonWorldCarver.Shape`.
#[derive(Clone, Debug, PartialEq)]
pub struct CanyonShape {
    pub distance_factor: FloatProvider,
    pub thickness: FloatProvider,
    pub width_smoothness: i32,
    pub horizontal_radius_factor: FloatProvider,
    pub vertical_radius_default_factor: f32,
    pub vertical_radius_center_factor: f32,
    pub y_scale: FloatProvider,
}

impl CanyonShape {
    /// `CanyonWorldCarver.Shape.CODEC`.
    fn from_json(value: &Value) -> LoadResult<CanyonShape> {
        let f = Fields::new(value)?;
        Ok(CanyonShape {
            distance_factor: f.required("distance_factor", FloatProvider::from_json)?,
            thickness: f.required("thickness", FloatProvider::from_json)?,
            width_smoothness: f.required("width_smoothness", parse_positive_int)?,
            horizontal_radius_factor: f
                .required("horizontal_radius_factor", FloatProvider::from_json)?,
            vertical_radius_default_factor: f
                .required("vertical_radius_default_factor", parse_f32)?,
            vertical_radius_center_factor: f
                .required("vertical_radius_center_factor", parse_f32)?,
            y_scale: f.required("y_scale", FloatProvider::from_json)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanyonWorldCarver {
    pub probability: f32,
    pub y: HeightProvider,
    pub vertical_rotation: FloatProvider,
    pub shape: CanyonShape,
}

impl CanyonWorldCarver {
    /// `CanyonWorldCarver.MAP_CODEC`.
    pub(crate) fn from_fields(f: &Fields<'_>) -> LoadResult<CanyonWorldCarver> {
        Ok(CanyonWorldCarver {
            probability: f.required("probability", |v| parse_float_range(v, 0.0, 1.0))?,
            y: f.required("y", HeightProvider::from_json)?,
            vertical_rotation: f.required("vertical_rotation", FloatProvider::from_json)?,
            shape: f.required("shape", CanyonShape::from_json)?,
        })
    }

    pub fn from_json(value: &Value) -> LoadResult<CanyonWorldCarver> {
        CanyonWorldCarver::from_fields(&Fields::new(value)?)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla `doCarve` parameter list"
    )]
    fn do_carve(
        &self,
        context: &WorldGenerationContext,
        chunk_pos: ChunkPos,
        tunnel_seed: i64,
        mut x: f64,
        mut y: f64,
        mut z: f64,
        thickness: f32,
        mut horizontal_rotation: f32,
        mut vertical_rotation: f32,
        step: i32,
        distance: i32,
        y_scale: f64,
        output: &mut dyn CarverOutput,
    ) {
        let mut random = create_thread_local_instance(tunnel_seed);
        let width_factor_per_height = self.init_width_factors(context, &mut random);
        let mut y_rota = 0.0f32;
        let mut x_rota = 0.0f32;

        for current_step in step..distance {
            let mut horizontal_radius = 1.5
                + f64::from(
                    mth::sin(f64::from(current_step as f32 * PI / distance as f32)) * thickness,
                );
            let mut vertical_radius = horizontal_radius * y_scale;
            horizontal_radius *= f64::from(self.shape.horizontal_radius_factor.sample(&mut random));
            vertical_radius = self.update_vertical_radius(
                &mut random,
                vertical_radius,
                distance as f32,
                current_step as f32,
            );
            let xc = mth::cos(f64::from(vertical_rotation));
            let xs = mth::sin(f64::from(vertical_rotation));
            x += f64::from(mth::cos(f64::from(horizontal_rotation)) * xc);
            y += f64::from(xs);
            z += f64::from(mth::sin(f64::from(horizontal_rotation)) * xc);
            vertical_rotation *= 0.7f32;
            vertical_rotation += x_rota * 0.05f32;
            horizontal_rotation += y_rota * 0.05f32;
            x_rota *= 0.8f32;
            y_rota *= 0.5f32;
            x_rota += (random.next_float() - random.next_float()) * random.next_float() * 2.0f32;
            y_rota += (random.next_float() - random.next_float()) * random.next_float() * 4.0f32;
            if random.next_int_bounded(4) != 0 {
                if !can_reach(chunk_pos, x, z, current_step, distance, thickness) {
                    return;
                }
                carve_ellipsoid(
                    chunk_pos,
                    x,
                    y,
                    z,
                    horizontal_radius,
                    vertical_radius,
                    output,
                    &|xd, yd, zd, world_y| {
                        should_skip(context, &width_factor_per_height, xd, yd, zd, world_y)
                    },
                );
            }
        }
    }

    fn init_width_factors(
        &self,
        context: &WorldGenerationContext,
        random: &mut dyn RandomSource,
    ) -> Vec<f32> {
        let depth = context.get_gen_depth();
        let mut width_factor_per_height = Vec::with_capacity(depth.max(0) as usize);
        let mut width_factor = 1.0f32;
        for y_index in 0..depth {
            if y_index == 0 || random.next_int_bounded(self.shape.width_smoothness) == 0 {
                width_factor = 1.0f32 + random.next_float() * random.next_float();
            }
            width_factor_per_height.push(width_factor * width_factor);
        }
        width_factor_per_height
    }

    fn update_vertical_radius(
        &self,
        random: &mut dyn RandomSource,
        vertical_radius: f64,
        distance: f32,
        current_step: f32,
    ) -> f64 {
        let vertical_multiplier = 1.0f32 - (0.5f32 - current_step / distance).abs() * 2.0f32;
        let factor = self.shape.vertical_radius_default_factor
            + self.shape.vertical_radius_center_factor * vertical_multiplier;
        f64::from(factor)
            * vertical_radius
            * f64::from(mth::random_between(random, 0.75f32, 1.0f32))
    }
}

/// `CanyonWorldCarver.shouldSkip`. Indexing panics out of range like the Java array access.
fn should_skip(
    context: &WorldGenerationContext,
    width_factor_per_height: &[f32],
    xd: f64,
    yd: f64,
    zd: f64,
    y: i32,
) -> bool {
    let y_index = y - context.get_min_gen_y();
    (xd * xd + zd * zd) * f64::from(width_factor_per_height[(y_index - 1) as usize]) + yd * yd / 6.0
        >= 1.0
}

impl WorldCarver for CanyonWorldCarver {
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
        let max_distance = (self.get_range() * 2 - 1) * 16;
        let x = f64::from(source_chunk_pos.get_block_x(random.next_int_bounded(16)));
        let y = self.y.sample(random, context);
        let z = f64::from(source_chunk_pos.get_block_z(random.next_int_bounded(16)));
        let horizontal_rotation = random.next_float() * TWO_PI;
        let vertical_rotation = self.vertical_rotation.sample(random);
        let y_scale = f64::from(self.shape.y_scale.sample(random));
        let thickness = self.shape.thickness.sample(random);
        let distance = (max_distance as f32 * self.shape.distance_factor.sample(random)) as i32;
        let tunnel_seed = random.next_long();
        self.do_carve(
            context,
            chunk_pos,
            tunnel_seed,
            x,
            f64::from(y),
            z,
            thickness,
            horizontal_rotation,
            vertical_rotation,
            0,
            distance,
            y_scale,
            output,
        );
        true
    }
}
