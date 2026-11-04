//! `RandomState`: the per-world, per-seed noise state. Instantiates registered noises from the world
//! seed's positional random and owns the density function compiler wired to them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mc_core::ident::Identifier;
use mc_core::random::{Algorithm, LegacyRandomSource, PositionalRandomFactory, RandomSource};

use mc_data::block::BlockState;

use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::{
    CompileContext, DensityBuffer, DensityFunction, DensityFunctionCompiler, DensityVolume,
    NoiseHolder, SamplerContext, ScopedDensityBuffer, SharedSampler,
};
use crate::material::MaterialSystem;
use crate::noise_router::NoiseRouter;
use crate::noise_settings::NoiseGeneratorSettings;
use crate::noises::{self, names};
use crate::synth::Noise;

/// State shared between a [`RandomState`] and its compile context.
struct Shared {
    seed: i64,
    use_legacy_random: bool,
    random: Arc<dyn PositionalRandomFactory>,
    registries: Arc<WorldgenRegistries>,
    noise_instances: Mutex<HashMap<Identifier, Arc<dyn Noise>>>,
    positional_randoms: Mutex<HashMap<Identifier, Arc<dyn PositionalRandomFactory>>>,
}

impl Shared {
    fn new_legacy_instance(&self, seed_offset: i64) -> LegacyRandomSource {
        LegacyRandomSource::new(self.seed.wrapping_add(seed_offset))
    }

    /// `getOrCreateNoise` for an already resolved noise: `Noises.instantiate`.
    fn get_or_create_noise(&self, noise: &NoiseHolder) -> Arc<dyn Noise> {
        let mut instances = self
            .noise_instances
            .lock()
            .expect("noise instance lock poisoned");
        instances
            .entry(noise.key.clone())
            .or_insert_with(|| {
                noise
                    .value
                    .create(&mut *self.random.from_hash_of_identifier(&noise.key))
            })
            .clone()
    }
}

/// `RandomState.createCompileContext`: the `CompileContext` handed to `compileSampler`.
struct RandomStateCompileContext {
    shared: Arc<Shared>,
}

impl CompileContext for RandomStateCompileContext {
    fn create_noise_sampler(&self, parameters: &NoiseHolder) -> Arc<dyn Noise> {
        let shared = &self.shared;
        if parameters.key == noises::key(names::TEMPERATURE_NETHER) {
            parameters
                .value
                .create_for_legacy_nether_biome(&mut shared.new_legacy_instance(0))
        } else if parameters.key == noises::key(names::VEGETATION_NETHER) {
            parameters
                .value
                .create_for_legacy_nether_biome(&mut shared.new_legacy_instance(1))
        } else {
            shared.get_or_create_noise(parameters)
        }
    }

    fn create_random(&self, seed: &Identifier) -> Box<dyn RandomSource> {
        let shared = &self.shared;
        if shared.use_legacy_random && *seed == noises::key("terrain") {
            Box::new(shared.new_legacy_instance(0))
        } else {
            shared.random.from_hash_of_identifier(seed)
        }
    }

    fn create_end_island_random(&self) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(self.shared.seed))
    }
}

pub struct RandomState {
    shared: Arc<Shared>,
    router: NoiseRouter,
    compiler: DensityFunctionCompiler,
    default_block: BlockState,
    sea_level: i32,
    surface_system: OnceLock<MaterialSystem>,
}

impl RandomState {
    /// `RandomState.create(noises, seed, settings)`.
    pub fn create(
        registries: Arc<WorldgenRegistries>,
        seed: i64,
        settings: &NoiseGeneratorSettings,
    ) -> RandomState {
        let default_block = settings
            .default_block
            .resolve()
            .expect("noise settings default_block names a valid block state");
        Self::create_with(
            registries,
            seed,
            settings.legacy_random_source,
            default_block,
            settings.sea_level,
            settings.noise_router.clone(),
        )
    }

    /// `RandomState.create(noises, seed, useLegacyRandom, ..., noiseRouter)`.
    pub fn create_with(
        registries: Arc<WorldgenRegistries>,
        seed: i64,
        use_legacy_random: bool,
        default_block: BlockState,
        sea_level: i32,
        router: NoiseRouter,
    ) -> RandomState {
        let algorithm = if use_legacy_random {
            Algorithm::Legacy
        } else {
            Algorithm::Xoroshiro
        };
        let random: Arc<dyn PositionalRandomFactory> =
            Arc::from(algorithm.new_instance(seed).fork_positional());
        let shared = Arc::new(Shared {
            seed,
            use_legacy_random,
            random,
            registries,
            noise_instances: Mutex::new(HashMap::new()),
            positional_randoms: Mutex::new(HashMap::new()),
        });
        let compiler = DensityFunctionCompiler::new(Box::new(RandomStateCompileContext {
            shared: shared.clone(),
        }));
        RandomState {
            shared,
            router,
            compiler,
            default_block,
            sea_level,
            surface_system: OnceLock::new(),
        }
    }

    /// `NoiseGeneratorSettings.defaultBlock()` this state was created with.
    pub fn default_block(&self) -> BlockState {
        self.default_block
    }

    /// `NoiseGeneratorSettings.seaLevel()` this state was created with.
    pub fn sea_level(&self) -> i32 {
        self.sea_level
    }

    /// `createClimateSampler(context)`.
    pub fn create_climate_sampler(self: &Arc<Self>, context: SamplerContext) -> crate::biome::climate::Sampler {
        self.router.create_climate_sampler(self.samplers_with_context(context))
    }

    /// `surfaceSystem()`: the `MaterialSystem` of this state (vanilla builds it in the constructor from the
    /// default block, sea level, `router.chunkSurfaceLevel()` and the world's positional random; built on
    /// first use here, which is unobservable as noise instances only depend on the seed).
    ///
    /// # Panics
    /// If the noise registry lacks one of the noises the material system needs (`surface`, `clay_bands_offset`, …).
    pub fn surface_system(&self) -> &MaterialSystem {
        self.surface_system.get_or_init(|| {
            MaterialSystem::new(
                self,
                self.default_block,
                self.sea_level,
                self.router.chunk_surface_level.clone(),
                self.shared.random.clone(),
            )
            .expect("noise registry contains the noises of the material system")
        })
    }

    /// `samplersWithContext(context)`: samplers of any function bound to `context`.
    pub fn samplers_with_context(self: &Arc<Self>, context: SamplerContext) -> DensitySamplerSet {
        DensitySamplerSet {
            random_state: self.clone(),
            context,
        }
    }

    pub fn router(&self) -> &NoiseRouter {
        &self.router
    }

    /// `seed()`.
    pub fn seed(&self) -> i64 {
        self.shared.seed
    }

    /// `getSampler(function)`: the (memoized) compiled sampler of a function.
    pub fn get_sampler(&self, function: &DensityFunction) -> SharedSampler {
        self.compiler.get_sampler(function)
    }

    /// `sampleBlockValueUncached(function, x, y, z)`.
    pub fn sample_block_value_uncached(
        &self,
        function: &DensityFunction,
        x: i32,
        y: i32,
        z: i32,
    ) -> f32 {
        self.get_sampler(function)
            .sample_value(&SamplerContext::empty_uncached(), x, y, z)
    }

    /// `getOrCreateNoise(key)`: the noise registered under `key` instantiated from this world's seed.
    pub fn get_or_create_noise(&self, key: &Identifier) -> Result<Arc<dyn Noise>, LoadError> {
        let holder = self.shared.registries.noise(key)?;
        Ok(self.shared.get_or_create_noise(&holder))
    }

    /// `getOrCreateRandomFactory(name)`.
    pub fn get_or_create_random_factory(
        &self,
        name: &Identifier,
    ) -> Arc<dyn PositionalRandomFactory> {
        let mut randoms = self
            .shared
            .positional_randoms
            .lock()
            .expect("positional random lock poisoned");
        randoms
            .entry(name.clone())
            .or_insert_with(|| {
                Arc::from(
                    self.shared
                        .random
                        .from_hash_of_identifier(name)
                        .fork_positional(),
                )
            })
            .clone()
    }
}

/// `DensitySamplerSet`: a [`SamplerContext`] (user fields, caches, buffer pool) together with the
/// [`RandomState`] that compiles samplers for it (`RandomState.samplersWithContext`).
pub struct DensitySamplerSet {
    random_state: Arc<RandomState>,
    context: SamplerContext,
}

impl DensitySamplerSet {
    pub fn random_state(&self) -> &Arc<RandomState> {
        &self.random_state
    }

    pub fn context(&self) -> &SamplerContext {
        &self.context
    }

    /// `get(function)`: the compiled sampler of `function` bound to this set's context.
    pub fn get(&self, function: &DensityFunction) -> BoundSampler<'_> {
        BoundSampler {
            sampler: self.random_state.get_sampler(function),
            context: &self.context,
        }
    }

    /// `sampleValue(function, x, y, z)`.
    pub fn sample_value(&self, function: &DensityFunction, x: i32, y: i32, z: i32) -> f32 {
        self.get(function).sample_value(x, y, z)
    }
}

/// `DensitySampler.Bound`: a sampler together with the context it samples in.
pub struct BoundSampler<'a> {
    sampler: SharedSampler,
    context: &'a SamplerContext,
}

impl<'a> BoundSampler<'a> {
    pub(crate) fn new(sampler: SharedSampler, context: &'a SamplerContext) -> BoundSampler<'a> {
        BoundSampler { sampler, context }
    }

    /// `sampleVolume(out, volume)`.
    pub fn sample_volume(&self, out: &mut DensityBuffer, volume: &DensityVolume) {
        self.sampler.sample_volume(self.context, out, volume);
    }

    /// `sampleVolume(volume)`: samples into a buffer borrowed from the context's pool.
    pub fn sample_volume_scoped(&self, volume: &DensityVolume) -> ScopedDensityBuffer<'_> {
        let mut buffer = self.context.acquire_buffer(volume);
        self.sampler.sample_volume(self.context, &mut buffer, volume);
        buffer
    }

    /// `sampleValue(x, y, z)`.
    pub fn sample_value(&self, x: i32, y: i32, z: i32) -> f32 {
        self.sampler.sample_value(self.context, x, y, z)
    }
}
