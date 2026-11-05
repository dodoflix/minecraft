//! `ChickenSoundVariant` (the `chicken_sound_variant` registry): the adult and baby sound sets of a mob.

use std::sync::LazyLock;

use mc_data::SoundEvent;
use mc_dfu::{Codec, record};
use serde_json::Value;

use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::world::entity::variant::{decode_variant, sound_event_codec};

/// `ChickenSoundVariants.CLASSIC`.
pub const CLASSIC: &str = "minecraft:classic";
/// `ChickenSoundVariants.PICKY`.
pub const PICKY: &str = "minecraft:picky";

/// `ChickenSoundVariant.ChickenSoundSet`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChickenSoundSet {
    pub ambient_sound: SoundEvent,
    pub hurt_sound: SoundEvent,
    pub death_sound: SoundEvent,
    pub step_sound: SoundEvent,
}

impl ChickenSoundSet {
    /// `ChickenSoundSet.CODEC`.
    pub fn codec() -> Codec<ChickenSoundSet> {
        static C: LazyLock<Codec<ChickenSoundSet>> = LazyLock::new(|| {
            record!(ChickenSoundSet {
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

/// `ChickenSoundVariant`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChickenSoundVariant {
    pub adult_sounds: ChickenSoundSet,
    pub baby_sounds: ChickenSoundSet,
}

impl ChickenSoundVariant {
    /// `DIRECT_CODEC` = `NETWORK_CODEC`.
    pub fn codec() -> Codec<ChickenSoundVariant> {
        static C: LazyLock<Codec<ChickenSoundVariant>> = LazyLock::new(|| {
            record!(ChickenSoundVariant {
                adult_sounds: ChickenSoundSet::codec().field_of("adult_sounds"),
                baby_sounds: ChickenSoundSet::codec().field_of("baby_sounds"),
            })
            .codec()
        });
        C.clone()
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    decode_variant(&ChickenSoundVariant::codec(), &ChickenSoundVariant::codec(), json, ctx)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::CHICKEN_SOUND_VARIANT, decode };
