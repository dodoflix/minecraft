//! `ZombieNautilusVariant` (the `ZOMBIE_NAUTILUS_VARIANT` registry): model + textures and spawn conditions.

use std::sync::LazyLock;

use mc_dfu::{Codec, MapCodec, record};
use serde_json::Value;

use mc_game_core::registry::codec::RegistryDecodeContext;
use mc_game_core::registry::decoders::{
    DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder,
};
use mc_game_core::registry::keys;
use mc_game_core::world::entity::variant::model_and_texture::{ModelAndTexture, model_type};
use mc_game_core::world::entity::variant::{
    PriorityProvider, Selector, SpawnPrioritySelectors, decode_variant,
};

/// `TemperatureVariants.TEMPERATE` (the default variant).
pub const TEMPERATE: &str = mc_game_core::world::entity::animal::temperature_variants::TEMPERATE;

model_type! {
    /// `ZombieNautilusVariant.ModelType`.
    ModelType {
        Normal = "normal",
        Warm = "warm"
    }
}

/// `ZombieNautilusVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct ZombieNautilusVariant {
    /// `modelAndTexture`.
    pub model_and_texture: ModelAndTexture<ModelType>,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

impl PriorityProvider for ZombieNautilusVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<ZombieNautilusVariant> {
    record!(ZombieNautilusVariant {
        model_and_texture: ModelAndTexture::codec(&ModelType::codec(), ModelType::Normal),
        spawn_conditions: spawn_conditions,
    })
    .codec()
}

impl ZombieNautilusVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<ZombieNautilusVariant> {
        static C: LazyLock<Codec<ZombieNautilusVariant>> = LazyLock::new(|| {
            codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions"))
        });
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<ZombieNautilusVariant> {
        static C: LazyLock<Codec<ZombieNautilusVariant>> =
            LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &ZombieNautilusVariant::direct_codec(),
        &ZombieNautilusVariant::network_codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::ZOMBIE_NAUTILUS_VARIANT,
    decode,
};
