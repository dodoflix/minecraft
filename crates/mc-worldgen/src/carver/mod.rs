//! `net.minecraft.world.level.levelgen.carver`: `WorldCarver` (+ `carveEllipsoid`, `canReach`),
//! `CaveWorldCarver`, `CanyonWorldCarver` and the `worldgen/carver` registry loader.
//!
//! In this version carvers only write into a [`CarverOutput`] (the `CarvingMask`); replacing blocks is
//! `NoiseBasedChunkGenerator.applyCarvingMask`.

pub mod canyon;
pub mod cave;

use std::sync::Arc;

use serde_json::Value;

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::pos::ChunkPos;
use mc_core::random::RandomSource;
use mc_world::carving_mask::CarverOutput;

use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{LoadResult, parse_f32};
use crate::provider::dispatch_type;
use crate::world_generation_context::WorldGenerationContext;

pub use canyon::{CanyonShape, CanyonWorldCarver};
pub use cave::CaveWorldCarver;

pub const CARVER_REGISTRY: &str = "worldgen/carver";

/// `WorldCarver.CarveSkipChecker.shouldSkip(xd, yd, zd, y)`.
pub type CarveSkipChecker<'a> = dyn Fn(f64, f64, f64, i32) -> bool + 'a;

/// `WorldCarver` (the carver configuration is part of the carver, as in vanilla 26.3).
pub trait WorldCarver: Send + Sync {
    /// `getRange()`: how many chunks around the target chunk can contain tunnels reaching it.
    fn get_range(&self) -> i32 {
        4
    }

    fn carve(
        &self,
        context: &WorldGenerationContext,
        random: &mut dyn RandomSource,
        chunk_pos: ChunkPos,
        source_chunk_pos: ChunkPos,
        output: &mut dyn CarverOutput,
    ) -> bool;

    fn is_start_chunk(&self, random: &mut dyn RandomSource) -> bool;
}

/// `WorldCarver.carveEllipsoid`.
pub fn carve_ellipsoid(
    chunk_pos: ChunkPos,
    x: f64,
    y: f64,
    z: f64,
    horizontal_radius: f64,
    vertical_radius: f64,
    output: &mut dyn CarverOutput,
    skip_checker: &CarveSkipChecker<'_>,
) {
    let center_x = f64::from(chunk_pos.get_middle_block_x());
    let center_z = f64::from(chunk_pos.get_middle_block_z());
    let max_delta = 16.0 + horizontal_radius * 2.0;
    if (x - center_x).abs() > max_delta || (z - center_z).abs() > max_delta {
        return;
    }
    let chunk_min_x = chunk_pos.get_min_block_x();
    let chunk_min_z = chunk_pos.get_min_block_z();
    let min_x_index = (mth::floor_f64(x - horizontal_radius) - chunk_min_x - 1).max(0);
    let max_x_index = (mth::floor_f64(x + horizontal_radius) - chunk_min_x).min(15);
    let min_y = (mth::floor_f64(y - vertical_radius) - 1).max(output.min_y());
    let max_y = (mth::floor_f64(y + vertical_radius) + 1).min(output.max_y());
    let min_z_index = (mth::floor_f64(z - horizontal_radius) - chunk_min_z - 1).max(0);
    let max_z_index = (mth::floor_f64(z + horizontal_radius) - chunk_min_z).min(15);

    for x_index in min_x_index..=max_x_index {
        let world_x = chunk_pos.get_block_x(x_index);
        let xd = (f64::from(world_x) + 0.5 - x) / horizontal_radius;
        for z_index in min_z_index..=max_z_index {
            let world_z = chunk_pos.get_block_z(z_index);
            let zd = (f64::from(world_z) + 0.5 - z) / horizontal_radius;
            if xd * xd + zd * zd >= 1.0 {
                continue;
            }
            let mut world_y = max_y;
            while world_y > min_y {
                let yd = (f64::from(world_y) - 0.5 - y) / vertical_radius;
                if !skip_checker(xd, yd, zd, world_y) {
                    output.carve(x_index, world_y, z_index);
                }
                world_y -= 1;
            }
        }
    }
}

/// `WorldCarver.canReach`.
pub fn can_reach(
    chunk_pos: ChunkPos,
    x: f64,
    z: f64,
    current_step: i32,
    total_steps: i32,
    thickness: f32,
) -> bool {
    let x_mid = f64::from(chunk_pos.get_middle_block_x());
    let z_mid = f64::from(chunk_pos.get_middle_block_z());
    let xd = x - x_mid;
    let zd = z - z_mid;
    let remaining = f64::from(total_steps.wrapping_sub(current_step));
    let rr = f64::from(thickness + 2.0f32 + 16.0f32);
    xd * xd + zd * zd - remaining * remaining <= rr * rr
}

/// `Codec.floatRange(min, max)`.
pub(crate) fn parse_float_range(value: &Value, min: f32, max: f32) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if v >= min && v <= max {
        Ok(v)
    } else {
        Err(LoadError::new(format!(
            "Value {v:?} outside of range [{min:?}:{max:?}]"
        )))
    }
}

/// `WorldCarver.DIRECT_CODEC`: the `type` (`cave` | `canyon`) selects the carver's `MapCodec`.
pub fn carver_from_json(value: &Value) -> LoadResult<Arc<dyn WorldCarver>> {
    let (ty, fields) = dispatch_type(value)?;
    match ty.as_str() {
        "cave" => Ok(Arc::new(CaveWorldCarver::from_fields(&fields)?)),
        "canyon" => Ok(Arc::new(CanyonWorldCarver::from_fields(&fields)?)),
        other => Err(crate::provider::unknown_type(other)),
    }
}

/// A decoded `worldgen/carver` entry as stored in the registry memo (one `T` per registry).
struct CarverEntry(Arc<dyn WorldCarver>);

/// The registered carver `id`; every call for one id returns the same carver (a `Holder.Reference`).
pub fn load_carver(
    registries: &WorldgenRegistries,
    id: &Identifier,
) -> LoadResult<Arc<dyn WorldCarver>> {
    let entry = registries.memoized::<CarverEntry>(CARVER_REGISTRY, id, |json, _| {
        carver_from_json(json).map(CarverEntry)
    })?;
    Ok(entry.0.clone())
}

impl WorldgenRegistries {
    /// `Registries.CARVER` lookup, see [`load_carver`].
    pub fn carver(&self, id: &Identifier) -> LoadResult<Arc<dyn WorldCarver>> {
        load_carver(self, id)
    }
}
