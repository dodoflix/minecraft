//! `net.minecraft.world.level.levelgen.NoiseChunk`: the per-chunk holder of the density volume, the aquifer
//! and the caching sampler set (with the beardifier and blender samplers as user fields).

use std::rc::Rc;
use std::sync::Arc;

use mc_core::ident::Identifier;

use crate::aquifer::{self, Aquifer, FluidPicker};
use crate::beardifier::Beardifier;
use crate::blending::Blender;
use crate::densityfunction::sampler::{
    BEARDIFIER_KEY, BLENDER_ALPHA_KEY, BLENDER_KEY, BLENDER_OFFSET_KEY, SharedBlender,
    SharedSampler,
};
use crate::densityfunction::{ContextMap, DensityVolume, SamplerContext};
use crate::noise_settings::NoiseGeneratorSettings;
use crate::random_state::{DensitySamplerSet, RandomState};

/// `NoiseChunk`. Not `Send`: it owns a [`SamplerContext`] (per-thread caches).
pub struct NoiseChunk {
    volume: DensityVolume,
    aquifer: Box<dyn Aquifer>,
    caching_samplers: Rc<DensitySamplerSet>,
}

impl NoiseChunk {
    /// `new NoiseChunk(randomState, beardifier, settings, globalFluidPicker, blender, volume)`.
    pub fn new(
        random_state: &Arc<RandomState>,
        beardifier: Option<Beardifier>,
        settings: &NoiseGeneratorSettings,
        global_fluid_picker: FluidPicker,
        blender: &Blender,
        volume: DensityVolume,
    ) -> NoiseChunk {
        let mut sampler_user_fields = ContextMap::new();
        if let Some(beardifier) = beardifier {
            sampler_user_fields =
                sampler_user_fields.with(BEARDIFIER_KEY, Arc::new(beardifier) as SharedSampler);
        }
        if !blender.is_empty() {
            let blend_buffer = blender.blend_offset_and_factor(&volume);
            sampler_user_fields = sampler_user_fields
                .with(BLENDER_KEY, Arc::new(blender.clone()) as SharedBlender)
                .with(BLENDER_ALPHA_KEY, blend_buffer.create_alpha_sampler())
                .with(BLENDER_OFFSET_KEY, blend_buffer.create_offset_sampler());
        }

        let caching_samplers = Rc::new(
            random_state.samplers_with_context(
                SamplerContext::builder()
                    .set_user_fields(sampler_user_fields)
                    .enable_caches()
                    .build(),
            ),
        );
        let aquifer: Box<dyn Aquifer> = match &settings.aquifers {
            Some(config) => Box::new(config.create(
                caching_samplers.clone(),
                random_state.get_or_create_random_factory(
                    &Identifier::with_default_namespace("aquifer").expect("valid id"),
                ),
                &volume,
                global_fluid_picker,
            )),
            None => aquifer::create_disabled(global_fluid_picker),
        };
        NoiseChunk {
            volume,
            aquifer,
            caching_samplers,
        }
    }

    pub fn volume(&self) -> &DensityVolume {
        &self.volume
    }

    pub fn aquifer(&mut self) -> &mut dyn Aquifer {
        &mut *self.aquifer
    }

    pub fn caching_samplers(&self) -> &DensitySamplerSet {
        &self.caching_samplers
    }

    /// The shared handle of [`Self::caching_samplers`], for callers that need `&mut` access to the aquifer while sampling.
    pub fn caching_samplers_rc(&self) -> Rc<DensitySamplerSet> {
        self.caching_samplers.clone()
    }
}
