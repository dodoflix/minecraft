//! `CowVariant` (the `COW_VARIANT` registry): model + textures and spawn conditions.

use std::sync::LazyLock;

use mc_dfu::{Codec, MapCodec, record};
use serde_json::Value;

use mc_game_core::registry::codec::RegistryDecodeContext;
use mc_game_core::registry::decoders::{
    DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder,
};
use mc_game_core::registry::keys;
use mc_game_core::world::entity::variant::model_and_texture::{
    ModelAndTexture, ResourceTexture, model_type,
};
use mc_game_core::world::entity::variant::{
    PriorityProvider, Selector, SpawnPrioritySelectors, decode_variant,
};

/// `TemperatureVariants.TEMPERATE` (the default variant).
pub const TEMPERATE: &str = mc_game_core::world::entity::animal::temperature_variants::TEMPERATE;

model_type! {
    /// `CowVariant.ModelType`.
    ModelType {
        Normal = "normal",
        Cold = "cold",
        Warm = "warm"
    }
}

/// `CowVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct CowVariant {
    /// `modelAndTexture`.
    pub model_and_texture: ModelAndTexture<ModelType>,
    /// `babyTexture`.
    pub baby_texture: ResourceTexture,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

impl PriorityProvider for CowVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<CowVariant> {
    record!(CowVariant {
        model_and_texture: ModelAndTexture::codec(&ModelType::codec(), ModelType::Normal),
        baby_texture: ResourceTexture::codec().field_of("baby_asset_id"),
        spawn_conditions: spawn_conditions,
    })
    .codec()
}

impl CowVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<CowVariant> {
        static C: LazyLock<Codec<CowVariant>> = LazyLock::new(|| {
            codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions"))
        });
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<CowVariant> {
        static C: LazyLock<Codec<CowVariant>> =
            LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &CowVariant::direct_codec(),
        &CowVariant::network_codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::COW_VARIANT,
    decode,
};
