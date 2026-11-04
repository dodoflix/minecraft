//! `SpawnTargetPoint` and `NoiseSpawnFinder`: the world spawn search by climate parameters.

use serde_json::Value;

use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::quart_pos;

use super::climate::{Parameter, quantize_coord};
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::DensityFunction;
use crate::densityfunction::codec::{LoadResult, parse_identifier};
use crate::random_state::DensitySamplerSet;

/// `SpawnTargetPoint`: for each referenced density function the parameter range its value should lie in.
#[derive(Clone, Debug)]
pub struct SpawnTargetPoint {
    pub parameters: Vec<(DensityFunction, Parameter)>,
}

impl SpawnTargetPoint {
    /// `SpawnTargetPoint.CODEC`: an unbounded map `density function reference -> Climate.Parameter`.
    pub fn from_json(value: &Value, registries: &WorldgenRegistries) -> LoadResult<SpawnTargetPoint> {
        let map = value
            .as_object()
            .ok_or_else(|| LoadError::new("expected object"))?;
        let mut parameters = Vec::with_capacity(map.len());
        for (key, parameter) in map {
            let function = parse_identifier(&Value::String(key.clone()))
                .and_then(|id| registries.density_function(&id))
                .map_err(|e| e.at(key))?;
            parameters.push((
                function.value,
                Parameter::from_json(parameter).map_err(|e| e.at(key))?,
            ));
        }
        Ok(SpawnTargetPoint { parameters })
    }

    /// `sampleFitness(samplers, blockX, blockY, blockZ)`.
    pub fn sample_fitness(&self, samplers: &DensitySamplerSet, block_x: i32, block_y: i32, block_z: i32) -> i64 {
        let mut fitness = 0i64;
        for (function, parameter) in &self.parameters {
            let value = quantize_coord(samplers.sample_value(function, block_x, block_y, block_z));
            let distance = parameter.distance(value);
            fitness = fitness.wrapping_add(distance.wrapping_mul(distance));
        }
        fitness
    }
}

#[derive(Clone, Copy)]
struct SpawnResult {
    location: BlockPos,
    fitness: i64,
}

/// `NoiseSpawnFinder`.
pub struct NoiseSpawnFinder;

impl NoiseSpawnFinder {
    /// `findSpawnPosition(targetPoints, samplers)`.
    pub fn find_spawn_position(target_points: &[SpawnTargetPoint], samplers: &DensitySamplerSet) -> BlockPos {
        let mut result = Self::get_spawn_position_and_fitness(samplers, target_points, 0, 0);
        Self::radial_search(&mut result, samplers, target_points, 2048.0, 512.0);
        Self::radial_search(&mut result, samplers, target_points, 512.0, 32.0);
        result.location
    }

    fn radial_search(
        result: &mut SpawnResult,
        samplers: &DensitySamplerSet,
        target_points: &[SpawnTargetPoint],
        max_radius: f32,
        radius_increment: f32,
    ) {
        let mut angle = 0.0f32;
        let mut radius = radius_increment;
        let search_origin = result.location;
        while radius <= max_radius {
            let x = search_origin
                .x()
                .wrapping_add((f64::from(angle).sin() * f64::from(radius)) as i32);
            let z = search_origin
                .z()
                .wrapping_add((f64::from(angle).cos() * f64::from(radius)) as i32);
            let candidate = Self::get_spawn_position_and_fitness(samplers, target_points, x, z);
            if candidate.fitness < result.fitness {
                *result = candidate;
            }
            angle += radius_increment / radius;
            if f64::from(angle) > std::f64::consts::PI * 2.0 {
                angle = 0.0;
                radius += radius_increment;
            }
        }
    }

    fn get_spawn_position_and_fitness(
        samplers: &DensitySamplerSet,
        target_points: &[SpawnTargetPoint],
        block_x: i32,
        block_z: i32,
    ) -> SpawnResult {
        let quart_block_x = quart_pos::to_block(quart_pos::from_block(block_x));
        let quart_block_z = quart_pos::to_block(quart_pos::from_block(block_z));
        let mut min_fitness = i64::MAX;
        for point in target_points {
            min_fitness = min_fitness.min(point.sample_fitness(samplers, quart_block_x, 0, quart_block_z));
        }
        let distance_bias_to_world_origin = i64::from(block_x)
            .wrapping_mul(i64::from(block_x))
            .wrapping_add(i64::from(block_z).wrapping_mul(i64::from(block_z)));
        let fitness_with_distance = min_fitness
            .wrapping_mul(2048 * 2048)
            .wrapping_add(distance_bias_to_world_origin);
        SpawnResult {
            location: BlockPos::new(block_x, 0, block_z),
            fitness: fitness_with_distance,
        }
    }
}
