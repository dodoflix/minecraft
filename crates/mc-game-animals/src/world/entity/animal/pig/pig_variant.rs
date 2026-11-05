//! `PigVariant` (the `PIG_VARIANT` registry): model + textures and spawn conditions.

use std::sync::LazyLock;

use mc_dfu::{Codec, MapCodec, record};
use serde_json::Value;

use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::world::entity::variant::{PriorityProvider, Selector, SpawnPrioritySelectors, decode_variant};
use crate::world::entity::variant::model_and_texture::{ModelAndTexture, ResourceTexture, model_type};

/// `TemperatureVariants.TEMPERATE` (the default variant).
pub const TEMPERATE: &str = crate::world::entity::animal::temperature_variants::TEMPERATE;

model_type! {
    /// `PigVariant.ModelType`.
    ModelType {
        Normal = "normal",
        Cold = "cold"
    }
}

/// `PigVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct PigVariant {
    /// `modelAndTexture`.
    pub model_and_texture: ModelAndTexture<ModelType>,
    /// `babyTexture`.
    pub baby_texture: ResourceTexture,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

impl PriorityProvider for PigVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<PigVariant> {
    record!(PigVariant {
        model_and_texture: ModelAndTexture::codec(&ModelType::codec(), ModelType::Normal),
            baby_texture: ResourceTexture::codec().field_of("baby_asset_id"),
        spawn_conditions: spawn_conditions,
    })
    .codec()
}

impl PigVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<PigVariant> {
        static C: LazyLock<Codec<PigVariant>> = LazyLock::new(|| codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions")));
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<PigVariant> {
        static C: LazyLock<Codec<PigVariant>> = LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(&PigVariant::direct_codec(), &PigVariant::network_codec(), json, ctx)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::PIG_VARIANT, decode };
