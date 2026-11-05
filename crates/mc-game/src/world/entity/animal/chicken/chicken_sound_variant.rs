//! `ChickenSoundVariant` (`chicken_sound_variant` registry).

use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use mc_data::SoundEvent;
use serde_json::Value;

/// `ChickenSoundVariants.CLASSIC`.
pub const CLASSIC: &str = "minecraft:classic";

/// `ChickenSoundVariant.ChickenSoundSet`.
#[derive(Clone, Copy, Debug)]
pub struct ChickenSoundSet {
    pub ambient_sound: SoundEvent,
    pub hurt_sound: SoundEvent,
    pub death_sound: SoundEvent,
    pub step_sound: SoundEvent,
}

#[derive(Clone, Copy, Debug)]
pub struct ChickenSoundVariant {
    pub adult_sounds: ChickenSoundSet,
    pub baby_sounds: ChickenSoundSet,
}

fn sound(v: &Value, key: &str) -> Result<SoundEvent, String> {
    let name = v.get(key).and_then(Value::as_str).ok_or_else(|| format!("missing {key}"))?;
    SoundEvent::by_name(name).ok_or_else(|| format!("unknown sound event {name}"))
}

fn sound_set(v: &Value) -> Result<ChickenSoundSet, String> {
    Ok(ChickenSoundSet { ambient_sound: sound(v, "ambient_sound")?, hurt_sound: sound(v, "hurt_sound")?, death_sound: sound(v, "death_sound")?, step_sound: sound(v, "step_sound")? })
}

impl ChickenSoundVariant {
    pub fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self { adult_sounds: sound_set(json.get("adult_sounds").ok_or("missing adult_sounds")?)?, baby_sounds: sound_set(json.get("baby_sounds").ok_or("missing baby_sounds")?)? })
    }
}

fn decode(json: &Value, _ctx: &crate::registry::codec::RegistryDecodeContext) -> Result<DecodedEntry, String> {
    Ok(DecodedEntry::new(ChickenSoundVariant::from_json(json)?))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::CHICKEN_SOUND_VARIANT, decode };
