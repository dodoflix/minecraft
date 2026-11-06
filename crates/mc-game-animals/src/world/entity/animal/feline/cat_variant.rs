//! `CatVariant` (the `cat_variant` registry): textures and spawn conditions (biome / structure / moon brightness).

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

/// `CatVariant`.
#[derive(Clone, Debug, PartialEq)]
pub struct CatVariant {
    /// `adultAssetInfo`.
    pub adult_asset_info: ResourceTexture,
    /// `babyAssetInfo`.
    pub baby_asset_info: ResourceTexture,
    /// `spawnConditions`.
    pub spawn_conditions: SpawnPrioritySelectors,
}

impl CatVariant {
    /// `assetInfo(isBaby)`.
    pub fn asset_info(&self, is_baby: bool) -> &ResourceTexture {
        if is_baby {
            &self.baby_asset_info
        } else {
            &self.adult_asset_info
        }
    }
}

fn codec_with(spawn_conditions: MapCodec<SpawnPrioritySelectors>) -> Codec<CatVariant> {
    record!(CatVariant {
        adult_asset_info: ResourceTexture::default_field_codec(),
        baby_asset_info: ResourceTexture::codec().field_of("baby_asset_id"),
        spawn_conditions: spawn_conditions,
    })
    .codec()
}

impl PriorityProvider for CatVariant {
    fn selectors(&self) -> &[Selector] {
        &self.spawn_conditions.selectors
    }
}

impl CatVariant {
    /// `DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<CatVariant> {
        static C: LazyLock<Codec<CatVariant>> = LazyLock::new(|| {
            codec_with(SpawnPrioritySelectors::codec().field_of("spawn_conditions"))
        });
        C.clone()
    }

    /// `NETWORK_CODEC` (no spawn conditions).
    pub fn network_codec() -> Codec<CatVariant> {
        static C: LazyLock<Codec<CatVariant>> =
            LazyLock::new(|| codec_with(MapCodec::unit(SpawnPrioritySelectors::empty())));
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &CatVariant::direct_codec(),
        &CatVariant::network_codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::CAT_VARIANT,
    decode,
};
