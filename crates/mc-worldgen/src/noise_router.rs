//! `net.minecraft.world.level.levelgen.NoiseRouter`: the density functions a noise generator consumes.

use serde_json::Value;

use crate::biome::climate;
use crate::data::WorldgenRegistries;
use crate::densityfunction::DensityFunction;
use crate::densityfunction::codec::{Fields, LoadResult, density_function_from_json};
use crate::random_state::DensitySamplerSet;

#[derive(Clone, Debug)]
pub struct NoiseRouter {
    pub temperature: DensityFunction,
    pub vegetation: DensityFunction,
    pub continents: DensityFunction,
    pub erosion: DensityFunction,
    pub depth: DensityFunction,
    pub ridges: DensityFunction,
    pub chunk_surface_level: DensityFunction,
    pub final_density: DensityFunction,
}

impl NoiseRouter {
    /// `NoiseRouter.CODEC`: all eight fields are required `DensityFunction.CODEC`s.
    pub fn from_json(value: &Value, registries: &WorldgenRegistries) -> LoadResult<NoiseRouter> {
        let fields = Fields::new(value)?;
        let df = |name: &str| fields.required(name, |v| density_function_from_json(v, registries));
        Ok(NoiseRouter {
            temperature: df("temperature")?,
            vegetation: df("vegetation")?,
            continents: df("continents")?,
            erosion: df("erosion")?,
            depth: df("depth")?,
            ridges: df("ridges")?,
            chunk_surface_level: df("chunk_surface_level")?,
            final_density: df("final_density")?,
        })
    }

    /// `createClimateSampler(densitySamplers)`.
    pub fn create_climate_sampler(&self, samplers: DensitySamplerSet) -> climate::Sampler {
        let random_state = samplers.random_state().clone();
        let sampler = |function: &DensityFunction| random_state.get_sampler(function);
        let (temperature, humidity, continentalness, erosion, depth, weirdness) = (
            sampler(&self.temperature),
            sampler(&self.vegetation),
            sampler(&self.continents),
            sampler(&self.erosion),
            sampler(&self.depth),
            sampler(&self.ridges),
        );
        climate::Sampler::new(samplers, temperature, humidity, continentalness, erosion, depth, weirdness)
    }
}
