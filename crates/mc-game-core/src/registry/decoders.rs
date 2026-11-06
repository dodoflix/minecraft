//! Hook for typed decoding of data-driven registry entries.
//!
//! Every dynamic registry entry is loaded as JSON (validated to be an object like vanilla's codecs would require). A gameplay
//! module that owns a registry's Rust type registers a decoder from its own file:
//!
//! ```ignore
//! #[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
//! static WOLF_VARIANT_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
//!     registry: "minecraft:wolf_variant",
//!     decode: |json, ctx| Ok(DecodedEntry::new(WolfVariant::from_json(json, ctx)?)),
//! };
//! ```
//!
//! The decoder runs on the loading thread with a [`RegistryDecodeContext`] (holder/tag lookups); an `Err` becomes vanilla's
//! `Failed to parse <id> from pack <pack>` registry-loading error. The typed value is then available through
//! `Registry::get::<T>(id)` / `RegistryAccess::get::<T>(registry, id)`.

use std::any::Any;
use std::sync::Arc;

use mc_nbt::Tag;
use serde_json::Value;

pub use super::codec::{RegistryDecodeContext, RegistryLookup};

/// What a decoder produces for one entry.
pub struct DecodedEntry {
    /// The typed value.
    pub value: Arc<dyn Any + Send + Sync>,
    /// `elementCodec.encodeStart(NbtOps, value)` of the *network* codec of a synchronized registry (what `registry_data`
    /// carries for entries the client does not know). `None`: the raw JSON is converted instead (numbers narrowed like
    /// `JsonOps.convertTo`, which does not reproduce codec-specific number types).
    pub network: Option<Tag>,
}

impl DecodedEntry {
    pub fn new<T: Any + Send + Sync>(value: T) -> DecodedEntry {
        DecodedEntry {
            value: Arc::new(value),
            network: None,
        }
    }

    pub fn with_network<T: Any + Send + Sync>(value: T, network: Tag) -> DecodedEntry {
        DecodedEntry {
            value: Arc::new(value),
            network: Some(network),
        }
    }
}

/// A typed codec for one registry.
pub struct DynamicRegistryDecoder {
    /// Full registry key, e.g. `minecraft:worldgen/biome` (see [`super::keys`]).
    pub registry: &'static str,
    pub decode: fn(&Value, &RegistryDecodeContext) -> Result<DecodedEntry, String>,
}

#[linkme::distributed_slice]
pub static DYNAMIC_REGISTRY_DECODERS: [DynamicRegistryDecoder];

/// The decoder registered for `registry`, if any.
pub fn decoder_for(registry: &str) -> Option<&'static DynamicRegistryDecoder> {
    DYNAMIC_REGISTRY_DECODERS
        .iter()
        .find(|d| d.registry == registry)
}
