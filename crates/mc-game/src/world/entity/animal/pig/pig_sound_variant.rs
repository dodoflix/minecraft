//! `PigSoundVariant` (the `pig_sound_variant` registry): the adult and baby sound sets of a mob.

use std::sync::LazyLock;

use mc_data::SoundEvent;
use mc_dfu::{Codec, record};
use serde_json::Value;

use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::world::entity::variant::{decode_variant, sound_event_codec};

/// `PigSoundVariant.PigSoundSet`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PigSoundSet {
    pub ambient_sound: SoundEvent,
    pub hurt_sound: SoundEvent,
    pub death_sound: SoundEvent,
    pub step_sound: SoundEvent,
    pub eat_sound: SoundEvent,
}

impl PigSoundSet {
    /// `PigSoundSet.CODEC`.
    pub fn codec() -> Codec<PigSoundSet> {
        static C: LazyLock<Codec<PigSoundSet>> = LazyLock::new(|| {
            record!(PigSoundSet {
            ambient_sound: sound_event_codec().field_of("ambient_sound"),
            hurt_sound: sound_event_codec().field_of("hurt_sound"),
            death_sound: sound_event_codec().field_of("death_sound"),
            step_sound: sound_event_codec().field_of("step_sound"),
            eat_sound: sound_event_codec().field_of("eat_sound"),
            })
            .codec()
        });
        C.clone()
    }
}

/// `PigSoundVariant`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PigSoundVariant {
    pub adult_sounds: PigSoundSet,
    pub baby_sounds: PigSoundSet,
}

impl PigSoundVariant {
    /// `DIRECT_CODEC` = `NETWORK_CODEC`.
    pub fn codec() -> Codec<PigSoundVariant> {
        static C: LazyLock<Codec<PigSoundVariant>> = LazyLock::new(|| {
            record!(PigSoundVariant {
                adult_sounds: PigSoundSet::codec().field_of("adult_sounds"),
                baby_sounds: PigSoundSet::codec().field_of("baby_sounds"),
            })
            .codec()
        });
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(&PigSoundVariant::codec(), &PigSoundVariant::codec(), json, ctx)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::PIG_SOUND_VARIANT, decode };
