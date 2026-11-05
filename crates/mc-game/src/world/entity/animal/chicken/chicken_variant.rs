//! `ChickenVariant` (the `CHICKEN_VARIANT` registry): model + textures and spawn conditions.

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
    /// `ChickenVariant.ModelType`.
    ModelType {
        Normal = "normal",
        Cold = "cold"
    }
}

/// `ChickenVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChickenVariant {
    /// `modelAndTexture`.
    pub model_and_texture: ModelAndTexture<ModelType>,
    /// `babyTexture`.
    pub baby_texture: ResourceTexture,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

impl PriorityProvider for ChickenVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<ChickenVariant> {
    record!(ChickenVariant {
        model_and_texture: ModelAndTexture::codec(&ModelType::codec(), ModelType::Normal),
            baby_texture: ResourceTexture::codec().field_of("baby_asset_id"),
        spawn_conditions: spawn_conditions,
    })
    .codec()
}

impl ChickenVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<ChickenVariant> {
        static C: LazyLock<Codec<ChickenVariant>> = LazyLock::new(|| codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions")));
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<ChickenVariant> {
        static C: LazyLock<Codec<ChickenVariant>> = LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(&ChickenVariant::direct_codec(), &ChickenVariant::network_codec(), json, ctx)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::CHICKEN_VARIANT, decode };
