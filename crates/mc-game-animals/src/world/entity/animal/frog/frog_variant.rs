//! `FrogVariant` (the `frog_variant` registry): texture and spawn conditions.

use std::sync::LazyLock;

use mc_dfu::{Codec, MapCodec, record};
use serde_json::Value;

use mc_game_core::registry::codec::RegistryDecodeContext;
use mc_game_core::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use mc_game_core::registry::keys;
use mc_game_core::world::entity::variant::model_and_texture::ResourceTexture;
use mc_game_core::world::entity::variant::{PriorityProvider, Selector, SpawnPrioritySelectors, decode_variant};

/// `FrogVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct FrogVariant {
    /// `assetInfo`.
    pub asset_info: ResourceTexture,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<FrogVariant> {
    record!(FrogVariant { asset_info: ResourceTexture::default_field_codec(), spawn_conditions: spawn_conditions }).codec()
}

impl PriorityProvider for FrogVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

impl FrogVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<FrogVariant> {
        static C: LazyLock<Codec<FrogVariant>> = LazyLock::new(|| codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions")));
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<FrogVariant> {
        static C: LazyLock<Codec<FrogVariant>> = LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(&FrogVariant::direct_codec(), &FrogVariant::network_codec(), json, ctx)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::FROG_VARIANT, decode };
