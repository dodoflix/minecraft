//! `world.entity.decoration.painting.PaintingVariant`: the `painting_variant` registry.

use mc_core::ident::Identifier;
use mc_text::Component;
use serde_json::Value;

use super::get_i32;
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `PaintingVariant(width, height, assetId, title, author)`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintingVariant {
    pub width: i32,
    pub height: i32,
    pub asset_id: Identifier,
    pub title: Option<Component>,
    pub author: Option<Component>,
}

impl PaintingVariant {
    /// `area()`.
    pub fn area(&self) -> i32 {
        self.width * self.height
    }

    /// `DIRECT_STREAM_CODEC` value (what `ClientboundSetEntityData` carries for a direct holder).
    pub fn to_protocol(&self) -> mc_protocol::packets::play::PaintingVariant {
        mc_protocol::packets::play::PaintingVariant {
            width: self.width,
            height: self.height,
            asset_id: self.asset_id.clone(),
            title: self.title.clone(),
            author: self.author.clone(),
        }
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let network = super::network::network_schema(keys::PAINTING_VARIANT)
        .ok_or("missing painting variant schema")?
        .convert(json, ctx)?;
    let text = |key: &str| json.get(key).and_then(|v| Component::from_json(v).ok());
    let value = PaintingVariant {
        width: get_i32(json, "width").unwrap_or(1),
        height: get_i32(json, "height").unwrap_or(1),
        asset_id: json
            .get("asset_id")
            .and_then(Value::as_str)
            .and_then(Identifier::try_parse)
            .ok_or("invalid asset_id")?,
        title: text("title"),
        author: text("author"),
    };
    Ok(DecodedEntry::with_network(value, network))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::PAINTING_VARIANT,
    decode,
};
