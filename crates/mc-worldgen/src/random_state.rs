//! `RandomState`: the per-world, per-seed noise state. Instantiates registered noises from the world
//! seed's positional random and owns the density function compiler wired to them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mc_core::ident::Identifier;
use mc_core::random::{Algorithm, LegacyRandomSource, PositionalRandomFactory, RandomSource};

use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::{
    CompileContext, DensityFunction, DensityFunctionCompiler, NoiseHolder, SamplerContext,
    SharedSampler,
};
use crate::noise_router::NoiseRouter;
use crate::noise_settings::NoiseGeneratorSettings;
use crate::noises::{self, names};
use crate::synth::Noise;

/// State shared between a [`RandomState`] and its compile context.
struct Shared {
    seed: i64,
    use_legacy_random: bool,
    random: Box<dyn PositionalRandomFactory>,
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
}

impl RandomState {
    /// `RandomState.create(noises, seed, settings)`.
    pub fn create(
        registries: Arc<WorldgenRegistries>,
        seed: i64,
        settings: &NoiseGeneratorSettings,
    ) -> RandomState {
        Self::create_with(
            registries,
            seed,
            settings.legacy_random_source,
            settings.noise_router.clone(),
        )
    }

    /// `RandomState.create(noises, seed, useLegacyRandom, ..., noiseRouter)`.
    pub fn create_with(
        registries: Arc<WorldgenRegistries>,
        seed: i64,
        use_legacy_random: bool,
        router: NoiseRouter,
    ) -> RandomState {
        let algorithm = if use_legacy_random {
            Algorithm::Legacy
        } else {
            Algorithm::Xoroshiro
        };
        let random = algorithm.new_instance(seed).fork_positional();
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
