//! `world.damagesource.DamageType`: the `damage_type` registry.

use mc_nbt::Tag;
use serde_json::Value;

use super::{get_f32, get_str};
use crate::registry::codec::{RegistryDecodeContext, Schema, default, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `DamageScaling`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageScaling {
    Never,
    WhenCausedByLivingNonPlayer,
    Always,
}

/// `DamageEffects` (the hurt sound/effect family).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageEffects {
    Hurt,
    Thorns,
    Drowning,
    Burning,
    Poking,
    Freezing,
}

/// `DeathMessageType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeathMessageType {
    Default,
    FallVariants,
    IntentionalGameDesign,
}

/// `DamageType`.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageType {
    pub msg_id: String,
    pub scaling: DamageScaling,
    pub exhaustion: f32,
    pub effects: DamageEffects,
    pub death_message_type: DeathMessageType,
}

fn schema() -> Schema {
    Schema::Record(vec![
        req("message_id", Schema::String),
        req(
            "scaling",
            Schema::Enum(&["never", "when_caused_by_living_non_player", "always"]),
        ),
        req("exhaustion", Schema::Float),
        default(
            "effects",
            Schema::Enum(&[
                "hurt", "thorns", "drowning", "burning", "poking", "freezing",
            ]),
            Value::from("hurt"),
        ),
        default(
            "death_message_type",
            Schema::Enum(&["default", "fall_variants", "intentional_game_design"]),
            Value::from("default"),
        ),
    ])
}

impl DamageType {
    /// `DIRECT_CODEC` decode of validated JSON; the NBT is the same codec's encoding.
    pub fn from_json(
        json: &Value,
        ctx: &RegistryDecodeContext,
    ) -> Result<(DamageType, Tag), String> {
        let tag = schema().convert(json, ctx)?;
        let value = DamageType {
            msg_id: get_str(json, "message_id").unwrap_or_default().to_owned(),
            scaling: match get_str(json, "scaling") {
                Some("never") => DamageScaling::Never,
                Some("always") => DamageScaling::Always,
                _ => DamageScaling::WhenCausedByLivingNonPlayer,
            },
            exhaustion: get_f32(json, "exhaustion").unwrap_or(0.0),
            effects: match get_str(json, "effects") {
                Some("thorns") => DamageEffects::Thorns,
                Some("drowning") => DamageEffects::Drowning,
                Some("burning") => DamageEffects::Burning,
                Some("poking") => DamageEffects::Poking,
                Some("freezing") => DamageEffects::Freezing,
                _ => DamageEffects::Hurt,
            },
            death_message_type: match get_str(json, "death_message_type") {
                Some("fall_variants") => DeathMessageType::FallVariants,
                Some("intentional_game_design") => DeathMessageType::IntentionalGameDesign,
                _ => DeathMessageType::Default,
            },
        };
        Ok((value, tag))
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let (value, tag) = DamageType::from_json(json, ctx)?;
    Ok(DecodedEntry::with_network(value, tag))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::DAMAGE_TYPE,
    decode,
};
