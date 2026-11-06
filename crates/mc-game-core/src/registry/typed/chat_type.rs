//! `network.chat.ChatType` / `ChatTypeDecoration`: the `chat_type` registry.

use mc_nbt::Tag;
use mc_text::Style;
use serde_json::Value;

use super::get_str;
use crate::registry::codec::{RegistryDecodeContext, Schema, default, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `ChatTypeDecoration.Parameter`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatParameter {
    Sender,
    Target,
    Content,
}

/// `ChatTypeDecoration`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTypeDecoration {
    pub translation_key: String,
    pub parameters: Vec<ChatParameter>,
    pub style: Style,
}

/// `ChatType`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatType {
    pub chat: ChatTypeDecoration,
    pub narration: ChatTypeDecoration,
}

fn decoration_schema() -> Schema {
    Schema::Record(vec![
        req("translation_key", Schema::String),
        req(
            "parameters",
            Schema::list_of(Schema::Enum(&["sender", "target", "content"])),
        ),
        default("style", Schema::Style, Value::Object(Default::default())),
    ])
}

fn schema() -> Schema {
    Schema::Record(vec![
        req("chat", decoration_schema()),
        req("narration", decoration_schema()),
    ])
}

fn decoration(json: &Value) -> Result<ChatTypeDecoration, String> {
    Ok(ChatTypeDecoration {
        translation_key: get_str(json, "translation_key")
            .unwrap_or_default()
            .to_owned(),
        parameters: json
            .get("parameters")
            .and_then(Value::as_array)
            .map(|l| {
                l.iter()
                    .map(|p| match p.as_str() {
                        Some("target") => ChatParameter::Target,
                        Some("content") => ChatParameter::Content,
                        _ => ChatParameter::Sender,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        style: match json.get("style") {
            Some(s) => Style::from_json(s).map_err(|e| e.to_string())?,
            None => Style::EMPTY,
        },
    })
}

impl ChatType {
    /// `DIRECT_CODEC` decode; the NBT is the same codec's encoding.
    pub fn from_json(
        json: &Value,
        ctx: &crate::registry::codec::RegistryDecodeContext,
    ) -> Result<(ChatType, Tag), String> {
        let tag = schema().convert(json, ctx)?;
        let chat = decoration(json.get("chat").unwrap_or(&Value::Null))?;
        let narration = decoration(json.get("narration").unwrap_or(&Value::Null))?;
        Ok((ChatType { chat, narration }, tag))
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let (value, tag) = ChatType::from_json(json, ctx)?;
    Ok(DecodedEntry::with_network(value, tag))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::CHAT_TYPE,
    decode,
};
