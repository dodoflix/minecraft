//! `net.minecraft.world.level.levelgen.NoiseRouter`: the density functions a noise generator consumes.

use serde_json::Value;

use crate::data::WorldgenRegistries;
use crate::densityfunction::DensityFunction;
use crate::densityfunction::codec::{Fields, LoadResult, density_function_from_json};

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
}
