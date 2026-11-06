//! `CowSoundVariant` (the `cow_sound_variant` registry): the sounds of a mob.

use std::sync::LazyLock;

use mc_data::SoundEvent;
use mc_dfu::{Codec, record};
use serde_json::Value;

use mc_game_core::registry::codec::RegistryDecodeContext;
use mc_game_core::registry::decoders::{
    DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder,
};
use mc_game_core::registry::keys;
use mc_game_core::world::entity::variant::{decode_variant, sound_event_codec};

/// `CowSoundVariant`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CowSoundVariant {
    pub ambient_sound: SoundEvent,
    pub hurt_sound: SoundEvent,
    pub death_sound: SoundEvent,
    pub step_sound: SoundEvent,
}

impl CowSoundVariant {
    /// `DIRECT_CODEC` = `NETWORK_CODEC`.
    pub fn codec() -> Codec<CowSoundVariant> {
        static C: LazyLock<Codec<CowSoundVariant>> = LazyLock::new(|| {
            record!(CowSoundVariant {
                ambient_sound: sound_event_codec().field_of("ambient_sound"),
                hurt_sound: sound_event_codec().field_of("hurt_sound"),
                death_sound: sound_event_codec().field_of("death_sound"),
                step_sound: sound_event_codec().field_of("step_sound"),
            })
            .codec()
        });
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &CowSoundVariant::codec(),
        &CowSoundVariant::codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::COW_SOUND_VARIANT,
    decode,
};
