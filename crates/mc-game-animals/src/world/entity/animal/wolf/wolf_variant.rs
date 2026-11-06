//! `WolfVariant` (the `wolf_variant` registry): wild / tame / angry textures of adult and baby and spawn conditions.

use std::sync::LazyLock;

use mc_dfu::{Codec, MapCodec, record};
use serde_json::Value;

use mc_game_core::registry::codec::RegistryDecodeContext;
use mc_game_core::registry::decoders::{
    DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder,
};
use mc_game_core::registry::keys;
use mc_game_core::world::entity::variant::model_and_texture::ResourceTexture;
use mc_game_core::world::entity::variant::{
    PriorityProvider, Selector, SpawnPrioritySelectors, decode_variant,
};

/// `WolfVariant.AssetInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct WolfVariantAssetInfo {
    pub wild: ResourceTexture,
    pub tame: ResourceTexture,
    pub angry: ResourceTexture,
}

impl WolfVariantAssetInfo {
    /// `AssetInfo.CODEC`.
    pub fn codec() -> Codec<WolfVariantAssetInfo> {
        static C: LazyLock<Codec<WolfVariantAssetInfo>> = LazyLock::new(|| {
            record!(WolfVariantAssetInfo {
                wild: ResourceTexture::codec().field_of("wild"),
                tame: ResourceTexture::codec().field_of("tame"),
                angry: ResourceTexture::codec().field_of("angry"),
            })
            .codec()
        });
        C.clone()
    }
}

/// `WolfVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct WolfVariant {
    /// `adultInfo`.
    pub adult_info: WolfVariantAssetInfo,
    /// `babyInfo`.
    pub baby_info: WolfVariantAssetInfo,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<WolfVariant> {
    record!(WolfVariant {
        adult_info: WolfVariantAssetInfo::codec().field_of("assets"),
        baby_info: WolfVariantAssetInfo::codec().field_of("baby_assets"),
        spawn_conditions: spawn_conditions,
    })
    .codec()
}

impl PriorityProvider for WolfVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

impl WolfVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<WolfVariant> {
        static C: LazyLock<Codec<WolfVariant>> = LazyLock::new(|| {
            codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions"))
        });
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<WolfVariant> {
        static C: LazyLock<Codec<WolfVariant>> =
            LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &WolfVariant::direct_codec(),
        &WolfVariant::network_codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::WOLF_VARIANT,
    decode,
};
