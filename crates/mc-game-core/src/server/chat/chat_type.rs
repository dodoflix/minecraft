//! `net.minecraft.network.chat.ChatType` / `ChatType.Bound` / `ChatTypeDecoration`.
//!
//! The data-only forms (`ChatType`, `ChatTypeDecoration`, parameters) are the wire types of `mc_protocol`; this module
//! adds their behavior (`decorate`), the vanilla bootstrap values, the data-pack JSON codec, and `Bound`.

use mc_core::ident::Identifier;
use mc_protocol::codec::Holder;
use mc_protocol::packets::play::{ChatType, ChatTypeBound, ChatTypeDecoration, ChatTypeDecorationParameter};
use mc_text::{ChatFormatting, Component, Style, TranslatableArg};
use serde_json::Value;
use std::sync::Arc;

pub const CHAT: &str = "minecraft:chat";
pub const SAY_COMMAND: &str = "minecraft:say_command";
pub const MSG_COMMAND_INCOMING: &str = "minecraft:msg_command_incoming";
pub const MSG_COMMAND_OUTGOING: &str = "minecraft:msg_command_outgoing";
pub const TEAM_MSG_COMMAND_INCOMING: &str = "minecraft:team_msg_command_incoming";
pub const TEAM_MSG_COMMAND_OUTGOING: &str = "minecraft:team_msg_command_outgoing";
pub const EMOTE_COMMAND: &str = "minecraft:emote_command";

fn decoration(key: &str, parameters: &[ChatTypeDecorationParameter], style: Style) -> ChatTypeDecoration {
    ChatTypeDecoration { translation_key: key.to_owned(), parameters: parameters.to_vec(), style }
}

use ChatTypeDecorationParameter::{Content, Sender, Target};

/// `ChatTypeDecoration.withSender`.
pub fn with_sender(key: &str) -> ChatTypeDecoration {
    decoration(key, &[Sender, Content], Style::EMPTY)
}

pub fn incoming_direct_message(key: &str) -> ChatTypeDecoration {
    let style = Style::EMPTY.with_color_formatting(Some(ChatFormatting::Gray)).with_italic(Some(true));
    decoration(key, &[Sender, Content], style)
}

pub fn outgoing_direct_message(key: &str) -> ChatTypeDecoration {
    let style = Style::EMPTY.with_color_formatting(Some(ChatFormatting::Gray)).with_italic(Some(true));
    decoration(key, &[Target, Content], style)
}

pub fn team_message(key: &str) -> ChatTypeDecoration {
    decoration(key, &[Target, Sender, Content], Style::EMPTY)
}

/// `ChatType.bootstrap`: the seven vanilla chat types in registration order.
pub fn bootstrap() -> Vec<(&'static str, ChatType)> {
    let narrate = || with_sender("chat.type.text.narrate");
    vec![
        (CHAT, ChatType { chat: with_sender("chat.type.text"), narration: narrate() }),
        (SAY_COMMAND, ChatType { chat: with_sender("chat.type.announcement"), narration: narrate() }),
        (MSG_COMMAND_INCOMING, ChatType { chat: incoming_direct_message("commands.message.display.incoming"), narration: narrate() }),
        (MSG_COMMAND_OUTGOING, ChatType { chat: outgoing_direct_message("commands.message.display.outgoing"), narration: narrate() }),
        (TEAM_MSG_COMMAND_INCOMING, ChatType { chat: team_message("chat.type.team.text"), narration: narrate() }),
        (TEAM_MSG_COMMAND_OUTGOING, ChatType { chat: team_message("chat.type.team.sent"), narration: narrate() }),
        (EMOTE_COMMAND, ChatType { chat: with_sender("chat.type.emote"), narration: with_sender("chat.type.emote") }),
    ]
}

fn parameter_from_name(name: &str) -> Option<ChatTypeDecorationParameter> {
    match name {
        "sender" => Some(Sender),
        "target" => Some(Target),
        "content" => Some(Content),
        _ => None,
    }
}

fn decoration_from_json(value: &Value) -> Result<ChatTypeDecoration, String> {
    let object = value.as_object().ok_or("chat type decoration must be an object")?;
    let translation_key = object.get("translation_key").and_then(Value::as_str).ok_or("missing translation_key")?.to_owned();
    let parameters = object
        .get("parameters")
        .and_then(Value::as_array)
        .ok_or("missing parameters")?
        .iter()
        .map(|p| p.as_str().and_then(parameter_from_name).ok_or_else(|| format!("Unknown chat type parameter: {p}")))
        .collect::<Result<Vec<_>, _>>()?;
    let style = match object.get("style") {
        Some(style) => Style::from_json(style).map_err(|e| e.to_string())?,
        None => Style::EMPTY,
    };
    Ok(ChatTypeDecoration { translation_key, parameters, style })
}

/// `ChatType.DIRECT_CODEC` (data-pack JSON).
pub fn chat_type_from_json(value: &Value) -> Result<ChatType, String> {
    let object = value.as_object().ok_or("chat type must be an object")?;
    Ok(ChatType {
        chat: decoration_from_json(object.get("chat").ok_or("missing chat")?)?,
        narration: decoration_from_json(object.get("narration").ok_or("missing narration")?)?,
    })
}

pub trait ChatTypeDecorationExt {
    /// `decorate(content, chatType)`.
    fn decorate(&self, content: &Component, bound: &Bound) -> Component;
}

impl ChatTypeDecorationExt for ChatTypeDecoration {
    fn decorate(&self, content: &Component, bound: &Bound) -> Component {
        let args = self
            .parameters
            .iter()
            .map(|p| {
                TranslatableArg::Component(match p {
                    Sender => bound.name.clone(),
                    Target => bound.target_name.clone().unwrap_or_else(Component::empty),
                    Content => content.clone(),
                })
            })
            .collect();
        Component::translatable(self.translation_key.clone(), args).with_style(&self.style)
    }
}

/// Resolves chat types of the (data-driven) `chat_type` registry: key -> (protocol id, definition).
pub trait ChatTypeLookup {
    fn chat_type(&self, key: &Identifier) -> Option<(i32, Arc<ChatType>)>;
}

/// `ChatType.Bound`: the chat type holder (registry id + resolved value), the sender name and the target name.
#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub chat_type_id: i32,
    pub chat_type: Arc<ChatType>,
    pub name: Component,
    pub target_name: Option<Component>,
}

impl Bound {
    /// `ChatType.bind(key, registryAccess, name)`: `None` where vanilla's `getOrThrow` would throw.
    pub fn bind(lookup: &dyn ChatTypeLookup, chat_type: &str, name: Component) -> Option<Bound> {
        let key = Identifier::try_parse(chat_type)?;
        let (chat_type_id, chat_type) = lookup.chat_type(&key)?;
        Some(Bound { chat_type_id, chat_type, name, target_name: None })
    }

    pub fn decorate(&self, content: &Component) -> Component {
        self.chat_type.chat.decorate(content, self)
    }

    pub fn decorate_narration(&self, content: &Component) -> Component {
        self.chat_type.narration.decorate(content, self)
    }

    pub fn with_target_name(&self, target_name: Component) -> Bound {
        Bound { target_name: Some(target_name), ..self.clone() }
    }

    /// The wire form (`ChatType.Bound.STREAM_CODEC`).
    pub fn to_packet(&self) -> ChatTypeBound {
        ChatTypeBound { chat_type: Holder::Reference(self.chat_type_id), name: self.name.clone(), target_name: self.target_name.clone() }
    }
}
