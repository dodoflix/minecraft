//! `WolfSoundVariant` (the `wolf_sound_variant` registry): the adult and baby sound sets of a mob.

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

/// `WolfSoundVariant.WolfSoundSet`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WolfSoundSet {
    pub ambient_sound: SoundEvent,
    pub death_sound: SoundEvent,
    pub growl_sound: SoundEvent,
    pub hurt_sound: SoundEvent,
    pub pant_sound: SoundEvent,
    pub whine_sound: SoundEvent,
    pub step_sound: SoundEvent,
}

impl WolfSoundSet {
    /// `WolfSoundSet.CODEC`.
    pub fn codec() -> Codec<WolfSoundSet> {
        static C: LazyLock<Codec<WolfSoundSet>> = LazyLock::new(|| {
            record!(WolfSoundSet {
                ambient_sound: sound_event_codec().field_of("ambient_sound"),
                death_sound: sound_event_codec().field_of("death_sound"),
                growl_sound: sound_event_codec().field_of("growl_sound"),
                hurt_sound: sound_event_codec().field_of("hurt_sound"),
                pant_sound: sound_event_codec().field_of("pant_sound"),
                whine_sound: sound_event_codec().field_of("whine_sound"),
                step_sound: sound_event_codec().field_of("step_sound"),
            })
            .codec()
        });
        C.clone()
    }
}

/// `WolfSoundVariant`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WolfSoundVariant {
    pub adult_sounds: WolfSoundSet,
    pub baby_sounds: WolfSoundSet,
}

impl WolfSoundVariant {
    /// `DIRECT_CODEC` = `NETWORK_CODEC`.
    pub fn codec() -> Codec<WolfSoundVariant> {
        static C: LazyLock<Codec<WolfSoundVariant>> = LazyLock::new(|| {
            record!(WolfSoundVariant {
                adult_sounds: WolfSoundSet::codec().field_of("adult_sounds"),
                baby_sounds: WolfSoundSet::codec().field_of("baby_sounds"),
            })
            .codec()
        });
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &WolfSoundVariant::codec(),
        &WolfSoundVariant::codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::WOLF_SOUND_VARIANT,
    decode,
};
