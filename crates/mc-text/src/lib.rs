//! Vanilla 26.3 chat components: `net.minecraft.network.chat.*`, `ChatFormatting`, `locale.Language`.
//!
//! * [`Component`] / [`Style`] / [`ClickEvent`] / [`HoverEvent`]: the component tree.
//! * `Component::{from_json, to_json, from_nbt, to_nbt}`: `ComponentSerialization.CODEC` over
//!   `JsonOps` and `NbtOps` (same accepted shapes, same rejections).
//! * [`Language`]: the bundled `en_us.json`; translatable formatting follows `TranslatableContents`.
//! * [`legacy`]: `§` code helpers (`StringDecomposer.iterateFormatted`, plain/legacy string conversion).

pub mod chat_formatting;
pub mod codec;
pub mod component;
pub mod events;
mod java_util;
pub mod language;
pub mod legacy;
pub mod nbt_text;
mod ops;
pub mod profile;
pub mod style;
pub mod text_color;
pub mod util;
pub mod value;

pub use chat_formatting::ChatFormatting;
pub use codec::{
    CodecError, PayloadHooks, PayloadKind, Validators, set_payload_hooks, set_validators,
};
pub use component::{
    Component, ComponentContents, DataSource, NbtContents, ObjectContents, ObjectInfo,
    ScoreContents, ScoreName, SelectorContents, TranslatableArg, TranslatableContents,
};
pub use events::{
    ClickAction, ClickEvent, EntityTooltipInfo, HoverAction, HoverEvent, ItemStackTemplate,
};
pub use language::Language;
pub use style::{FontDescription, Style};
pub use text_color::TextColor;
pub use value::Num;

use mc_core::ident::Identifier;
use mc_nbt::Tag;
use serde_json::Value;

/// `minecraft:<path>` for constant, known-valid paths.
pub(crate) fn mc_id(path: &str) -> Identifier {
    Identifier::new("minecraft", path).expect("valid constant identifier")
}

impl Component {
    /// `ComponentSerialization.CODEC.parse(JsonOps.INSTANCE, json)`.
    pub fn from_json(json: &Value) -> Result<Component, CodecError> {
        codec::decode_component(json)
    }

    /// `ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, self)`. Fails like vanilla for
    /// components that cannot be serialised (e.g. `open_file` click events, sprite fonts).
    pub fn to_json(&self) -> Result<Value, CodecError> {
        codec::encode_component::<Value>(self)
    }

    /// `ComponentSerialization.CODEC.parse(NbtOps.INSTANCE, tag)` (the network format).
    pub fn from_nbt(tag: &Tag) -> Result<Component, CodecError> {
        codec::decode_component(tag)
    }

    /// `ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, self)`.
    pub fn to_nbt(&self) -> Result<Tag, CodecError> {
        codec::encode_component::<Tag>(self)
    }

    /// Parses JSON text (as `/tellraw` etc. would) and decodes it.
    pub fn from_json_str(json: &str) -> Result<Component, CodecError> {
        let value: Value = serde_json::from_str(json).map_err(|e| CodecError(e.to_string()))?;
        Component::from_json(&value)
    }

    /// Compact JSON text of [`Component::to_json`].
    pub fn to_json_string(&self) -> Result<String, CodecError> {
        Ok(self.to_json()?.to_string())
    }

    /// `ComponentSerialization.flatRestrictedCodec(max_flat_size).parse(NbtOps, tag)`: decoding fails
    /// when the component's compact JSON form is longer than `max_flat_size` characters.
    pub fn from_nbt_flat_restricted(
        tag: &Tag,
        max_flat_size: usize,
    ) -> Result<Component, CodecError> {
        let component = Component::from_nbt(tag)?;
        if component
            .to_json()
            .is_ok_and(|json| util::encodes_longer_than(&json, max_flat_size))
        {
            return Err(CodecError(format!(
                "Component was too large: greater than max size {max_flat_size}"
            )));
        }
        Ok(component)
    }

    /// Same as [`Component::from_nbt_flat_restricted`] for JSON input.
    pub fn from_json_flat_restricted(
        json: &Value,
        max_flat_size: usize,
    ) -> Result<Component, CodecError> {
        let component = Component::from_json(json)?;
        if component
            .to_json()
            .is_ok_and(|encoded| util::encodes_longer_than(&encoded, max_flat_size))
        {
            return Err(CodecError(format!(
                "Component was too large: greater than max size {max_flat_size}"
            )));
        }
        Ok(component)
    }
}

impl ClickEvent {
    /// `ClickEvent.CODEC.parse(JsonOps.INSTANCE, json)` (the `action` field selects the event type).
    pub fn from_json(json: &Value) -> Result<ClickEvent, CodecError> {
        codec::decode_click_event(json)
    }

    /// `ClickEvent.CODEC.encodeStart(JsonOps.INSTANCE, self)`.
    pub fn to_json(&self) -> Result<Value, CodecError> {
        codec::encode_click_event::<Value>(self)
    }

    /// `ClickEvent.CODEC.parse(NbtOps.INSTANCE, tag)`.
    pub fn from_nbt(tag: &Tag) -> Result<ClickEvent, CodecError> {
        codec::decode_click_event(tag)
    }

    /// `ClickEvent.CODEC.encodeStart(NbtOps.INSTANCE, self)`.
    pub fn to_nbt(&self) -> Result<Tag, CodecError> {
        codec::encode_click_event::<Tag>(self)
    }
}

impl Style {
    /// `Style.Serializer.CODEC.parse(JsonOps.INSTANCE, json)`.
    pub fn from_json(json: &Value) -> Result<Style, CodecError> {
        codec::decode_style_value(json)
    }

    /// `Style.Serializer.CODEC.encodeStart(JsonOps.INSTANCE, self)`.
    pub fn to_json(&self) -> Result<Value, CodecError> {
        codec::encode_style_value::<Value>(self)
    }

    /// `Style.Serializer.CODEC.parse(NbtOps.INSTANCE, tag)`.
    pub fn from_nbt(tag: &Tag) -> Result<Style, CodecError> {
        codec::decode_style_value(tag)
    }

    /// `Style.Serializer.CODEC.encodeStart(NbtOps.INSTANCE, self)`.
    pub fn to_nbt(&self) -> Result<Tag, CodecError> {
        codec::encode_style_value::<Tag>(self)
    }
}
