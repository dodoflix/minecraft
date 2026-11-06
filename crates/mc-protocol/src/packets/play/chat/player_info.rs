//! `ClientboundPlayerInfoUpdatePacket` / `ClientboundPlayerInfoRemovePacket`.
use crate::codec::EncodeResult;
use mc_text::Component;
use uuid::Uuid;

use super::RemoteChatSessionData;
use crate::codec::{
    DecodeBounded, DecodeResult, EnumSet, EnumValues, GameProfile, Property, read_count,
    write_varint,
};
use crate::packets::play::types::GameType;
use crate::{Decode, Encode, Packet};

/// `ClientboundPlayerInfoUpdatePacket.Action` (ordinal = bit in the action set = vanilla's id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientboundPlayerInfoUpdateAction {
    AddPlayer,
    InitializeChat,
    UpdateGameMode,
    UpdateListed,
    UpdateLatency,
    UpdateDisplayName,
    UpdateListOrder,
    UpdateHat,
}

impl ClientboundPlayerInfoUpdateAction {
    const ALL: [Self; 8] = [
        Self::AddPlayer,
        Self::InitializeChat,
        Self::UpdateGameMode,
        Self::UpdateListed,
        Self::UpdateLatency,
        Self::UpdateDisplayName,
        Self::UpdateListOrder,
        Self::UpdateHat,
    ];

    /// `ClientboundPlayerInfoUpdatePacket.createPlayerInitializing`'s action set.
    pub fn initializing() -> EnumSet<Self> {
        Self::ALL.into_iter().collect()
    }
}

impl EnumValues for ClientboundPlayerInfoUpdateAction {
    const COUNT: usize = 8;
    fn ordinal(self) -> usize {
        self as usize
    }
    fn from_ordinal(i: usize) -> Self {
        Self::ALL[i]
    }
}

/// `ClientboundPlayerInfoUpdatePacket.Entry`. Only the fields of the packet's actions are on the wire; the others hold
/// the decode defaults of vanilla's `EntryBuilder` (see [`new`](Self::new)).
#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundPlayerInfoUpdateEntry {
    pub profile_id: Uuid,
    /// `ADD_PLAYER` (required when that action is set). Only name and properties are sent; the id is `profile_id`.
    pub profile: Option<GameProfile>,
    pub listed: bool,
    pub latency: i32,
    pub game_mode: GameType,
    pub display_name: Option<Component>,
    pub show_hat: bool,
    pub list_order: i32,
    pub chat_session: Option<RemoteChatSessionData>,
}

impl ClientboundPlayerInfoUpdateEntry {
    /// `EntryBuilder` defaults.
    pub fn new(profile_id: Uuid) -> Self {
        Self {
            profile_id,
            profile: None,
            listed: false,
            latency: 0,
            game_mode: GameType::Survival,
            display_name: None,
            show_hat: false,
            list_order: 0,
            chat_session: None,
        }
    }
}

/// `ClientboundPlayerInfoUpdatePacket`
#[derive(Debug, Clone, PartialEq, Packet)]
#[packet(state = play, flow = clientbound, name = "player_info_update")]
pub struct ClientboundPlayerInfoUpdatePacket {
    pub actions: EnumSet<ClientboundPlayerInfoUpdateAction>,
    pub entries: Vec<ClientboundPlayerInfoUpdateEntry>,
}

impl Encode for ClientboundPlayerInfoUpdatePacket {
    /// `ADD_PLAYER` without a profile is vanilla's `Objects.requireNonNull` `NullPointerException`.
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        use ClientboundPlayerInfoUpdateAction as A;
        self.actions.encode(out)?;
        write_varint(out, self.entries.len() as i32);
        for e in &self.entries {
            e.profile_id.encode(out)?;
            for action in self.actions.iter() {
                match action {
                    A::AddPlayer => {
                        let profile = e.profile.as_ref().ok_or_else(|| {
                            crate::codec::EncodeError::msg("java.lang.NullPointerException")
                        })?;
                        crate::codec::write_utf(out, &profile.name, 16)?;
                        crate::codec::write_properties(out, &profile.properties)?;
                    }
                    A::InitializeChat => e.chat_session.encode(out)?,
                    A::UpdateGameMode => e.game_mode.encode(out)?,
                    A::UpdateListed => e.listed.encode(out)?,
                    A::UpdateLatency => write_varint(out, e.latency),
                    A::UpdateDisplayName => e.display_name.encode(out)?,
                    A::UpdateListOrder => write_varint(out, e.list_order),
                    A::UpdateHat => e.show_hat.encode(out)?,
                }
            }
        }
        Ok(())
    }
}

impl Decode for ClientboundPlayerInfoUpdatePacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        use ClientboundPlayerInfoUpdateAction as A;
        let actions = EnumSet::<A>::decode(input)?;
        let count = read_count(input, i32::MAX as usize)?;
        let mut entries = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let mut e = ClientboundPlayerInfoUpdateEntry::new(Uuid::decode(input)?);
            for action in actions.iter() {
                match action {
                    A::AddPlayer => {
                        let name = String::decode_bounded(input, 16)?;
                        let n = read_count(input, 16)?;
                        let mut properties = Vec::with_capacity(n);
                        for _ in 0..n {
                            properties.push(Property {
                                name: String::decode_bounded(input, 64)?,
                                value: String::decode_bounded(input, 32767)?,
                                signature: Option::<String>::decode_bounded(input, 1024)?,
                            });
                        }
                        e.profile = Some(GameProfile {
                            id: e.profile_id,
                            name,
                            properties,
                        });
                    }
                    A::InitializeChat => e.chat_session = Decode::decode(input)?,
                    A::UpdateGameMode => e.game_mode = Decode::decode(input)?,
                    A::UpdateListed => e.listed = Decode::decode(input)?,
                    A::UpdateLatency => e.latency = crate::codec::read_varint(input)?,
                    A::UpdateDisplayName => e.display_name = Decode::decode(input)?,
                    A::UpdateListOrder => e.list_order = crate::codec::read_varint(input)?,
                    A::UpdateHat => e.show_hat = Decode::decode(input)?,
                }
            }
            entries.push(e);
        }
        Ok(Self { actions, entries })
    }
}

/// `ClientboundPlayerInfoRemovePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_info_remove")]
pub struct ClientboundPlayerInfoRemovePacket {
    pub profile_ids: Vec<Uuid>,
}
