//! `CatSoundVariant` (the `cat_sound_variant` registry): the adult and baby sound sets of a mob.

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

/// `CatSoundVariant.CatSoundSet`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatSoundSet {
    pub ambient_sound: SoundEvent,
    pub stray_ambient_sound: SoundEvent,
    pub hiss_sound: SoundEvent,
    pub hurt_sound: SoundEvent,
    pub death_sound: SoundEvent,
    pub eat_sound: SoundEvent,
    pub beg_for_food_sound: SoundEvent,
    pub purr_sound: SoundEvent,
    pub purreow_sound: SoundEvent,
}

impl CatSoundSet {
    /// `CatSoundSet.CODEC`.
    pub fn codec() -> Codec<CatSoundSet> {
        static C: LazyLock<Codec<CatSoundSet>> = LazyLock::new(|| {
            record!(CatSoundSet {
                ambient_sound: sound_event_codec().field_of("ambient_sound"),
                stray_ambient_sound: sound_event_codec().field_of("stray_ambient_sound"),
                hiss_sound: sound_event_codec().field_of("hiss_sound"),
                hurt_sound: sound_event_codec().field_of("hurt_sound"),
                death_sound: sound_event_codec().field_of("death_sound"),
                eat_sound: sound_event_codec().field_of("eat_sound"),
                beg_for_food_sound: sound_event_codec().field_of("beg_for_food_sound"),
                purr_sound: sound_event_codec().field_of("purr_sound"),
                purreow_sound: sound_event_codec().field_of("purreow_sound"),
            })
            .codec()
        });
        C.clone()
    }
}

/// `CatSoundVariant`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatSoundVariant {
    pub adult_sounds: CatSoundSet,
    pub baby_sounds: CatSoundSet,
}

impl CatSoundVariant {
    /// `DIRECT_CODEC` = `NETWORK_CODEC`.
    pub fn codec() -> Codec<CatSoundVariant> {
        static C: LazyLock<Codec<CatSoundVariant>> = LazyLock::new(|| {
            record!(CatSoundVariant {
                adult_sounds: CatSoundSet::codec().field_of("adult_sounds"),
                baby_sounds: CatSoundSet::codec().field_of("baby_sounds"),
            })
            .codec()
        });
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(
        &CatSoundVariant::codec(),
        &CatSoundVariant::codec(),
        json,
        ctx,
    )
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::CAT_SOUND_VARIANT,
    decode,
};
