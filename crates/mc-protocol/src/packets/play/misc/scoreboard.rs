//! Scoreboard packets: objectives, scores, teams, display slots.
use mc_text::{Component, Style};

use crate::codec::{
    DecodeError, DecodeResult, NbtLimit, decode_tag_with, encode_tag, read_varint, write_varint,
};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

id_enum!(
    /// `DisplaySlot` (`BY_ID`: VarInt id, unknown = `LIST`)
    DisplaySlot {
        List = 0, Sidebar = 1, BelowName = 2, TeamBlack = 3, TeamDarkBlue = 4, TeamDarkGreen = 5, TeamDarkAqua = 6,
        TeamDarkRed = 7, TeamDarkPurple = 8, TeamGold = 9, TeamGray = 10, TeamDarkGray = 11, TeamBlue = 12,
        TeamGreen = 13, TeamAqua = 14, TeamRed = 15, TeamLightPurple = 16, TeamYellow = 17, TeamWhite = 18
    } out_of_bounds = ZERO
);
id_enum!(
    /// `TeamColor` (`STREAM_CODEC`: VarInt id, unknown = black)
    TeamColor {
        Black = 0, DarkBlue = 1, DarkGreen = 2, DarkAqua = 3, DarkRed = 4, DarkPurple = 5, Gold = 6, Gray = 7,
        DarkGray = 8, Blue = 9, Green = 10, Aqua = 11, Red = 12, LightPurple = 13, Yellow = 14, White = 15
    } out_of_bounds = ZERO
);
id_enum!(
    /// `Team.Visibility` (`STREAM_CODEC`: VarInt id, unknown = `ALWAYS`)
    TeamVisibility { Always = 0, Never = 1, HideForOtherTeams = 2, HideForOwnTeam = 3 } out_of_bounds = ZERO
);
id_enum!(
    /// `Team.CollisionRule` (`STREAM_CODEC`: VarInt id, unknown = `ALWAYS`)
    TeamCollisionRule { Always = 0, Never = 1, PushOtherTeams = 2, PushOwnTeam = 3 } out_of_bounds = ZERO
);

/// `ObjectiveCriteria.RenderType` (`writeEnum`: ordinal VarInt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 2")]
pub enum ObjectiveRenderType {
    Integer,
    Hearts,
}

/// `NumberFormat` (`NumberFormatTypes.STREAM_CODEC`: `number_format_type` registry id, then the format's own payload).
#[derive(Debug, Clone, PartialEq)]
pub enum NumberFormat {
    /// `BlankFormat`
    Blank,
    /// `StyledFormat` (the style as trusted NBT)
    Styled(Style),
    /// `FixedFormat`
    Fixed(Component),
}

impl Encode for NumberFormat {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Blank => write_varint(out, 0),
            Self::Styled(style) => {
                write_varint(out, 1);
                match style.to_nbt() {
                    Ok(tag) => encode_tag(&tag, out),
                    Err(e) => panic!("Failed to encode: {e} {style}"),
                }
            }
            Self::Fixed(value) => {
                write_varint(out, 2);
                value.encode(out)
            }
        }
    }
}
impl Decode for NumberFormat {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        match read_varint(input)? {
            0 => Ok(Self::Blank),
            1 => {
                let tag = decode_tag_with(input, NbtLimit::UNLIMITED)?
                    .ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
                Style::from_nbt(&tag)
                    .map(Self::Styled)
                    .map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {tag}")))
            }
            2 => Component::decode(input).map(Self::Fixed),
            id => Err(DecodeError::Message(format!("No value with id {id}"))),
        }
    }
}

/// `ClientboundSetDisplayObjectivePacket` (an empty name clears the slot).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_display_objective")]
pub struct ClientboundSetDisplayObjectivePacket {
    pub slot: DisplaySlot,
    pub objective_name: String,
}

/// Display name, render type and number format of an objective, sent for `METHOD_ADD`/`METHOD_CHANGE`.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ObjectiveSettings {
    pub display_name: Component,
    pub render_type: ObjectiveRenderType,
    pub number_format: Option<NumberFormat>,
}

/// `ClientboundSetObjectivePacket.method` with the settings that go with it. Vanilla's reader accepts any method byte
/// (others carry no settings), so they are kept as [`Self::Other`].
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectiveMethod {
    Add(ObjectiveSettings),
    Remove,
    Change(ObjectiveSettings),
    Other(i8),
}

impl Encode for ObjectiveMethod {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Add(s) => {
                0i8.encode(out);
                s.encode(out)
            }
            Self::Remove => 1i8.encode(out),
            Self::Change(s) => {
                2i8.encode(out);
                s.encode(out)
            }
            Self::Other(m) => m.encode(out),
        }
    }
}
impl Decode for ObjectiveMethod {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(match i8::decode(input)? {
            0 => Self::Add(Decode::decode(input)?),
            1 => Self::Remove,
            2 => Self::Change(Decode::decode(input)?),
            m => Self::Other(m),
        })
    }
}

/// `ClientboundSetObjectivePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_objective")]
pub struct ClientboundSetObjectivePacket {
    pub objective_name: String,
    pub method: ObjectiveMethod,
}

/// `ClientboundSetScorePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_score")]
pub struct ClientboundSetScorePacket {
    pub owner: String,
    pub objective_name: String,
    #[mc(var)]
    pub score: i32,
    pub display: Option<Component>,
    pub number_format: Option<NumberFormat>,
}

/// `ClientboundResetScorePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "reset_score")]
pub struct ClientboundResetScorePacket {
    pub owner: String,
    pub objective_name: Option<String>,
}

/// `ClientboundSetPlayerTeamPacket.Parameters`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TeamParameters {
    pub display_name: Component,
    pub player_prefix: Component,
    pub player_suffix: Component,
    pub name_tag_visibility: TeamVisibility,
    pub collision_rule: TeamCollisionRule,
    pub color: Option<TeamColor>,
    /// `PlayerTeam.packOptions` (bit 1 friendly fire, bit 2 see friendly invisibles).
    pub options: i8,
}

/// `ClientboundSetPlayerTeamPacket` method with its payload: parameters for add/change, player names for
/// add/join/leave (`METHOD_ADD` = 0 .. `METHOD_LEAVE` = 4). Other method bytes decode to [`Self::Other`] (no payload).
#[derive(Debug, Clone, PartialEq)]
pub enum TeamMethod {
    Add {
        parameters: TeamParameters,
        players: Vec<String>,
    },
    Remove,
    Change {
        parameters: TeamParameters,
    },
    Join {
        players: Vec<String>,
    },
    Leave {
        players: Vec<String>,
    },
    Other(i8),
}

impl Encode for TeamMethod {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Add {
                parameters,
                players,
            } => {
                0i8.encode(out);
                parameters.encode(out);
                players.encode(out)
            }
            Self::Remove => 1i8.encode(out),
            Self::Change { parameters } => {
                2i8.encode(out);
                parameters.encode(out)
            }
            Self::Join { players } => {
                3i8.encode(out);
                players.encode(out)
            }
            Self::Leave { players } => {
                4i8.encode(out);
                players.encode(out)
            }
            Self::Other(m) => m.encode(out),
        }
    }
}
impl Decode for TeamMethod {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(match i8::decode(input)? {
            0 => Self::Add {
                parameters: Decode::decode(input)?,
                players: Decode::decode(input)?,
            },
            1 => Self::Remove,
            2 => Self::Change {
                parameters: Decode::decode(input)?,
            },
            3 => Self::Join {
                players: Decode::decode(input)?,
            },
            4 => Self::Leave {
                players: Decode::decode(input)?,
            },
            m => Self::Other(m),
        })
    }
}

/// `ClientboundSetPlayerTeamPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_player_team")]
pub struct ClientboundSetPlayerTeamPacket {
    pub name: String,
    pub method: TeamMethod,
}
