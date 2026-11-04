//! Play packets, group `chat`: chat messages, titles, tab list, command suggestions, player info and the command tree
//! (see packets/play/mod.rs for the split).
//!
//! `Component`s inside clientbound packets are the *trusted* codec (`ComponentSerialization.TRUSTED_STREAM_CODEC`):
//! NBT without a size limit, which is what `Component`'s `Decode` does.
mod commands;
mod player_info;

use mc_text::{Component, Style};
use uuid::Uuid;

pub use commands::*;
pub use player_info::*;

use crate::auth::PublicKey;
use crate::codec::{BitSet, BitSetBytes, DecodeBounded, DecodeError, DecodeResult, FixedBitSet, Holder, Instant, MessageSignature};
use crate::codec::{decode_tag_with, encode_tag, read_varint, write_varint, NbtLimit};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

// ---- signed message types ---------------------------------------------------------------------------------------

/// `MessageSignature.Packed`: a reference into the 128-entry signature cache (`id`, sent as `id + 1`), or the full
/// signature (sent as `0` followed by the 256 bytes). `Cached(-1)` is not representable on the wire (it decodes as `Full`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSignaturePacked {
    Cached(i32),
    Full(MessageSignature),
}

impl Encode for MessageSignaturePacked {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Cached(id) => write_varint(out, id.wrapping_add(1)),
            Self::Full(signature) => {
                write_varint(out, 0);
                signature.encode(out)
            }
        }
    }
}
impl Decode for MessageSignaturePacked {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        match read_varint(input)?.wrapping_sub(1) {
            -1 => MessageSignature::decode(input).map(Self::Full),
            id => Ok(Self::Cached(id)),
        }
    }
}

/// `LastSeenMessages.Packed` (at most 20 entries).
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct LastSeenMessagesPacked {
    #[mc(max = 20)]
    pub entries: Vec<MessageSignaturePacked>,
}

/// `LastSeenMessages.Update`: `acknowledged` is the 20-bit set on the wire (3 bytes; the 4 padding bits are kept).
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct LastSeenMessagesUpdate {
    #[mc(var)]
    pub offset: i32,
    pub acknowledged: FixedBitSet<20>,
    /// `0` = ignore the checksum.
    pub checksum: i8,
}

/// `SignedMessageBody.Packed`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignedMessageBodyPacked {
    #[mc(max = 256)]
    pub content: String,
    pub time_stamp: Instant,
    pub salt: i64,
    pub last_seen: LastSeenMessagesPacked,
}

/// `FilterMask` (`FilterMask.STREAM_CODEC`): type id (VarInt, unknown = pass through), then the bit set (`byte[]` form)
/// for partially filtered messages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FilterMask {
    #[default]
    PassThrough,
    FullyFiltered,
    PartiallyFiltered(BitSet),
}

impl Encode for FilterMask {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::PassThrough => write_varint(out, 0),
            Self::FullyFiltered => write_varint(out, 1),
            Self::PartiallyFiltered(mask) => {
                write_varint(out, 2);
                BitSetBytes(mask.clone()).encode(out)
            }
        }
    }
}
impl Decode for FilterMask {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        match read_varint(input)? {
            1 => Ok(Self::FullyFiltered),
            2 => BitSetBytes::decode(input).map(|b| Self::PartiallyFiltered(b.0)),
            _ => Ok(Self::PassThrough),
        }
    }
}

// ---- chat types -------------------------------------------------------------------------------------------------

id_enum!(
    /// `ChatTypeDecoration.Parameter` (VarInt id, unknown = sender)
    ChatTypeDecorationParameter { Sender = 0, Target = 1, Content = 2 } out_of_bounds = ZERO
);

/// `Style.Serializer.TRUSTED_STREAM_CODEC`: the style as an unlimited NBT compound.
mod style_nbt {
    use super::*;

    pub fn encode(style: &Style, out: &mut Vec<u8>) {
        match style.to_nbt() {
            Ok(tag) => encode_tag(&tag, out),
            Err(e) => panic!("Failed to encode: {e} {style:?}"),
        }
    }

    pub fn decode(input: &mut &[u8]) -> DecodeResult<Style> {
        let tag = decode_tag_with(input, NbtLimit::UNLIMITED)?.ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
        Style::from_nbt(&tag).map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {tag}")))
    }
}

/// `ChatTypeDecoration`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ChatTypeDecoration {
    pub translation_key: String,
    pub parameters: Vec<ChatTypeDecorationParameter>,
    #[mc(with = style_nbt)]
    pub style: Style,
}

/// `ChatType.DIRECT_STREAM_CODEC` (the inline form of a chat type holder).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ChatType {
    pub chat: ChatTypeDecoration,
    pub narration: ChatTypeDecoration,
}

/// `ChatType.Bound`. `chat_type` is `ByteBufCodecs.holder(Registries.CHAT_TYPE, ..)`: the (data-driven) registry id of the
/// chat type, or an inline definition.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ChatTypeBound {
    pub chat_type: Holder<ChatType>,
    pub name: Component,
    pub target_name: Option<Component>,
}

// ---- chat session -----------------------------------------------------------------------------------------------

/// `ProfilePublicKey.Data.STREAM_CODEC`: expiry, X.509 RSA key (at most 512 bytes, validated like `Crypt.byteToPublicKey`)
/// and the key signature (at most 4096 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfilePublicKeyData {
    pub expires_at: Instant,
    pub key: PublicKey,
    #[mc(max = 4096)]
    pub key_signature: Vec<u8>,
}

/// `RemoteChatSession.Data`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteChatSessionData {
    pub session_id: Uuid,
    pub profile_public_key: ProfilePublicKeyData,
}

/// `ArgumentSignatures.Entry`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ArgumentSignaturesEntry {
    #[mc(max = 16)]
    pub name: String,
    pub signature: MessageSignature,
}

/// `ArgumentSignatures` (at most 8 entries).
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct ArgumentSignatures {
    #[mc(max = 8)]
    pub entries: Vec<ArgumentSignaturesEntry>,
}

// ---- clientbound ------------------------------------------------------------------------------------------------

/// `ClientboundPlayerChatPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_chat")]
pub struct ClientboundPlayerChatPacket {
    #[mc(var)]
    pub global_index: i32,
    pub sender: Uuid,
    #[mc(var)]
    pub index: i32,
    pub signature: Option<MessageSignature>,
    pub body: SignedMessageBodyPacked,
    pub unsigned_content: Option<Component>,
    pub filter_mask: FilterMask,
    pub chat_type: ChatTypeBound,
}

/// `ClientboundDisguisedChatPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "disguised_chat")]
pub struct ClientboundDisguisedChatPacket {
    pub message: Component,
    pub chat_type: ChatTypeBound,
}

/// `ClientboundDeleteChatPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "delete_chat")]
pub struct ClientboundDeleteChatPacket {
    pub message_signature: MessageSignaturePacked,
}

/// `ClientboundSystemChatPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "system_chat")]
pub struct ClientboundSystemChatPacket {
    pub content: Component,
    pub overlay: bool,
}

/// `ClientboundTabListPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "tab_list")]
pub struct ClientboundTabListPacket {
    pub header: Component,
    pub footer: Component,
}

/// `ClientboundClearTitlesPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "clear_titles")]
pub struct ClientboundClearTitlesPacket {
    pub reset_times: bool,
}

/// `ClientboundSetTitleTextPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_title_text")]
pub struct ClientboundSetTitleTextPacket {
    pub text: Component,
}

/// `ClientboundSetSubtitleTextPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_subtitle_text")]
pub struct ClientboundSetSubtitleTextPacket {
    pub text: Component,
}

/// `ClientboundSetActionBarTextPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_action_bar_text")]
pub struct ClientboundSetActionBarTextPacket {
    pub text: Component,
}

/// `ClientboundSetTitlesAnimationPacket` (ticks)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_titles_animation")]
pub struct ClientboundSetTitlesAnimationPacket {
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

/// `ClientboundCommandSuggestionsPacket.Entry`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ClientboundCommandSuggestionsEntry {
    pub text: String,
    pub tooltip: Option<Component>,
}

/// `ClientboundCommandSuggestionsPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "command_suggestions")]
pub struct ClientboundCommandSuggestionsPacket {
    #[mc(var)]
    pub id: i32,
    #[mc(var)]
    pub start: i32,
    #[mc(var)]
    pub length: i32,
    pub suggestions: Vec<ClientboundCommandSuggestionsEntry>,
}

id_enum!(
    /// `ClientboundCustomChatCompletionsPacket.Action` (VarInt id, unknown = add)
    ClientboundCustomChatCompletionsAction { Add = 0, Remove = 1, Set = 2 } out_of_bounds = ZERO
);

/// `ClientboundCustomChatCompletionsPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "custom_chat_completions")]
pub struct ClientboundCustomChatCompletionsPacket {
    pub action: ClientboundCustomChatCompletionsAction,
    pub entries: Vec<String>,
}

// ---- serverbound ------------------------------------------------------------------------------------------------

/// `ServerboundChatAckPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "chat_ack")]
pub struct ServerboundChatAckPacket {
    #[mc(var)]
    pub offset: i32,
}

/// `ServerboundChatCommandPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "chat_command")]
pub struct ServerboundChatCommandPacket {
    pub command: String,
}

/// `ServerboundChatCommandSignedPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "chat_command_signed")]
pub struct ServerboundChatCommandSignedPacket {
    pub command: String,
    pub time_stamp: Instant,
    pub salt: i64,
    pub argument_signatures: ArgumentSignatures,
    pub last_seen_messages: LastSeenMessagesUpdate,
}

/// `ServerboundChatPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "chat")]
pub struct ServerboundChatPacket {
    #[mc(max = 256)]
    pub message: String,
    pub time_stamp: Instant,
    pub salt: i64,
    pub signature: Option<MessageSignature>,
    pub last_seen_messages: LastSeenMessagesUpdate,
}

/// `ServerboundChatSessionUpdatePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "chat_session_update")]
pub struct ServerboundChatSessionUpdatePacket {
    pub chat_session: RemoteChatSessionData,
}

/// `ServerboundCommandSuggestionPacket`. `Decode` is `STREAM_CODEC` (command up to 32500 characters), used when the
/// player may use command blocks; for everyone else vanilla binds `CHAT_ONLY_STREAM_CODEC`, see
/// [`decode_chat_only`](Self::decode_chat_only).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "command_suggestion")]
pub struct ServerboundCommandSuggestionPacket {
    #[mc(var)]
    pub id: i32,
    #[mc(max = 32500)]
    pub command: String,
}

impl ServerboundCommandSuggestionPacket {
    /// `CHAT_ONLY_STREAM_CODEC.decode` (the command is limited to 256 characters). Vanilla wraps every
    /// `DecoderException` of this codec in a `SkipPacketDecoderException`: the packet is dropped, the connection kept.
    /// Those are the [`DecodeError::Message`] errors (string limits, short string bytes); `Eof` (an
    /// `IndexOutOfBoundsException`) and `VarIntTooBig` (a plain `RuntimeException`) still disconnect. Encoding is the
    /// same for both codecs.
    pub fn decode_chat_only(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self { id: read_varint(input)?, command: String::decode_bounded(input, 256)? })
    }
}
