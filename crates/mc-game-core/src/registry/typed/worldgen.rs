//! The `worldgen/noise`, `worldgen/density_function` and `worldgen/noise_settings` registries, decoded by
//! `mc_worldgen::data` against the entries of the stage being loaded.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_worldgen::densityfunction::{DensityFunctionHolder, NoiseHolder};
use mc_worldgen::noise_settings::NoiseGeneratorSettings;
use serde_json::Value;

use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

fn entry<'a>(ctx: &'a RegistryDecodeContext<'_>) -> Result<(&'a mc_worldgen::data::WorldgenRegistries, &'a Identifier), String> {
    match (ctx.worldgen, ctx.entry.as_ref()) {
        (Some(w), Some(e)) => Ok((w, e)),
        _ => Err("worldgen registries are not available in this context".to_owned()),
    }
}

fn decode_noise(_json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let (worldgen, id) = entry(ctx)?;
    let holder: NoiseHolder = worldgen.noise(id).map_err(|e| e.to_string())?;
    Ok(DecodedEntry::new(holder))
}

fn decode_density_function(_json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let (worldgen, id) = entry(ctx)?;
    let holder: DensityFunctionHolder = worldgen.density_function(id).map_err(|e| e.to_string())?;
    Ok(DecodedEntry::new(holder))
}

fn decode_noise_settings(_json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let (worldgen, id) = entry(ctx)?;
    let settings: NoiseGeneratorSettings = worldgen.noise_settings(id).map_err(|e| e.to_string())?;
    Ok(DecodedEntry { value: Arc::new(settings), network: None })
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static NOISE: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::NOISE, decode: decode_noise };

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DENSITY_FUNCTION: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::DENSITY_FUNCTION, decode: decode_density_function };

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static NOISE_SETTINGS: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::NOISE_SETTINGS, decode: decode_noise_settings };
