//! `ChickenVariant` (`chicken_variant` registry): spawn conditions + client assets.

use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::world::entity::variant::SpawnPrioritySelectors;
use serde_json::Value;

/// `ChickenVariants.TEMPERATE` (= `DEFAULT`).
pub const TEMPERATE: &str = "minecraft:temperate";

/// `ChickenVariant` (client-only assets are not kept: they go to the client verbatim through the registry data).
#[derive(Clone, Debug)]
pub struct ChickenVariant {
    pub spawn_conditions: SpawnPrioritySelectors,
}

impl ChickenVariant {
    /// `DIRECT_CODEC` decode.
    pub fn from_json(json: &Value) -> Result<Self, String> {
        let spawn_conditions = match json.get("spawn_conditions") {
            Some(v) => SpawnPrioritySelectors::from_json(v)?,
            None => return Err("missing spawn_conditions".to_string()),
        };
        Ok(Self { spawn_conditions })
    }
}

fn decode(json: &Value, _ctx: &crate::registry::codec::RegistryDecodeContext) -> Result<DecodedEntry, String> {
    Ok(DecodedEntry::new(ChickenVariant::from_json(json)?))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::CHICKEN_VARIANT, decode };
