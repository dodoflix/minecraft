//! Play packets, group `misc`: stats, boss bars, combat events, scoreboard, advancements, waypoints, tag queries.
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_data::{Block, CustomStat, EntityType, Item, StatType};
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use uuid::Uuid;

use crate::codec::{decode_tag_with, encode_tag, DecodeError, DecodeResult, NbtLimit, VarInt};
use crate::{Decode, Encode, Packet};

mod advancement;
mod scoreboard;
mod waypoint;

pub use advancement::*;
pub use scoreboard::*;
pub use waypoint::*;

/// `Stat<?>` (`Stat.STREAM_CODEC`): the stat type's registry id, then the id of the value in that type's own registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stat {
    Mined(Block),
    Crafted(Item),
    Used(Item),
    Broken(Item),
    PickedUp(Item),
    Dropped(Item),
    Killed(EntityType),
    KilledBy(EntityType),
    Custom(CustomStat),
}

impl Stat {
    /// Registry name of the `StatType` (`Stats.BLOCK_MINED` ...).
    fn type_name(&self) -> &'static str {
        match self {
            Self::Mined(_) => "mined",
            Self::Crafted(_) => "crafted",
            Self::Used(_) => "used",
            Self::Broken(_) => "broken",
            Self::PickedUp(_) => "picked_up",
            Self::Dropped(_) => "dropped",
            Self::Killed(_) => "killed",
            Self::KilledBy(_) => "killed_by",
            Self::Custom(_) => "custom",
        }
    }
}

impl Encode for Stat {
    fn encode(&self, out: &mut Vec<u8>) {
        StatType::by_name(self.type_name()).expect("vanilla stat type").encode(out);
        match self {
            Self::Mined(v) => v.encode(out),
            Self::Crafted(v) | Self::Used(v) | Self::Broken(v) | Self::PickedUp(v) | Self::Dropped(v) => v.encode(out),
            Self::Killed(v) | Self::KilledBy(v) => v.encode(out),
            Self::Custom(v) => v.encode(out),
        }
    }
}
impl Decode for Stat {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let stat_type = StatType::decode(input)?;
        Ok(match stat_type.name().strip_prefix("minecraft:").unwrap_or_default() {
            "mined" => Self::Mined(Decode::decode(input)?),
            "crafted" => Self::Crafted(Decode::decode(input)?),
            "used" => Self::Used(Decode::decode(input)?),
            "broken" => Self::Broken(Decode::decode(input)?),
            "picked_up" => Self::PickedUp(Decode::decode(input)?),
            "dropped" => Self::Dropped(Decode::decode(input)?),
            "killed" => Self::Killed(Decode::decode(input)?),
            "killed_by" => Self::KilledBy(Decode::decode(input)?),
            "custom" => Self::Custom(Decode::decode(input)?),
            other => return Err(DecodeError::Message(format!("Unknown stat type {other}"))),
        })
    }
}

/// `ClientboundAwardStatsPacket`: stat -> VarInt value, in the map's iteration order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "award_stats")]
pub struct ClientboundAwardStatsPacket {
    pub stats: Vec<(Stat, VarInt)>,
}

/// `BossEvent.BossBarColor` (`writeEnum`: ordinal VarInt; out of range = `ArrayIndexOutOfBoundsException`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 7")]
pub enum BossBarColor {
    Pink,
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    White,
}

/// `BossEvent.BossBarOverlay`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 5")]
pub enum BossBarOverlay {
    Progress,
    Notched6,
    Notched10,
    Notched12,
    Notched20,
}

/// The darken-screen / play-music / create-fog flag byte of a boss event (bits 1, 2, 4; other bits ignored on read).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BossEventProperties {
    pub darken_screen: bool,
    pub play_music: bool,
    pub create_world_fog: bool,
}

impl Encode for BossEventProperties {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.darken_screen as u8 | (self.play_music as u8) << 1 | (self.create_world_fog as u8) << 2)
    }
}
impl Decode for BossEventProperties {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let flags = u8::decode(input)?;
        Ok(Self { darken_screen: flags & 1 != 0, play_music: flags & 2 != 0, create_world_fog: flags & 4 != 0 })
    }
}

/// `ClientboundBossEventPacket.Operation` (the `OperationType` ordinal is the variant id).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 6")]
pub enum BossEventOperation {
    Add { name: Component, progress: f32, color: BossBarColor, overlay: BossBarOverlay, properties: BossEventProperties },
    Remove,
    UpdateProgress { progress: f32 },
    UpdateName { name: Component },
    UpdateStyle { color: BossBarColor, overlay: BossBarOverlay },
    UpdateProperties { properties: BossEventProperties },
}

/// `ClientboundBossEventPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "boss_event")]
pub struct ClientboundBossEventPacket {
    pub id: Uuid,
    pub operation: BossEventOperation,
}

/// `ClientboundPlayerCombatEndPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_combat_end")]
pub struct ClientboundPlayerCombatEndPacket {
    #[mc(var)]
    pub duration: i32,
}

/// `ClientboundPlayerCombatEnterPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_combat_enter")]
pub struct ClientboundPlayerCombatEnterPacket;

/// `ClientboundPlayerCombatKillPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_combat_kill")]
pub struct ClientboundPlayerCombatKillPacket {
    #[mc(var)]
    pub player_id: i32,
    pub message: Component,
}

/// `ClientboundSelectAdvancementsTabPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "select_advancements_tab")]
pub struct ClientboundSelectAdvancementsTabPacket {
    pub tab: Option<Identifier>,
}

/// `ClientboundServerDataPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "server_data")]
pub struct ClientboundServerDataPacket {
    pub motd: Component,
    pub icon_bytes: Option<Vec<u8>>,
}

/// `ClientboundSetExperiencePacket` (wire order: progress, level, total).
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_experience")]
pub struct ClientboundSetExperiencePacket {
    pub experience_progress: f32,
    #[mc(var)]
    pub experience_level: i32,
    #[mc(var)]
    pub total_experience: i32,
}

/// `ClientboundSetHealthPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_health")]
pub struct ClientboundSetHealthPacket {
    pub health: f32,
    #[mc(var)]
    pub food: i32,
    pub saturation: f32,
}

/// `FriendlyByteBuf.readNbt()`/`writeNbt` of a nullable compound: a bare `End` tag is `None`; other tag types are rejected.
mod nullable_compound {
    use super::*;

    pub fn encode(tag: &Option<CompoundTag>, out: &mut Vec<u8>) {
        match tag {
            Some(c) => encode_tag(&Tag::Compound(c.clone()), out),
            None => out.push(0),
        }
    }

    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<CompoundTag>> {
        match decode_tag_with(input, NbtLimit::DEFAULT)? {
            None => Ok(None),
            Some(Tag::Compound(c)) => Ok(Some(c)),
            Some(other) => Err(DecodeError::Message(format!("Not a compound tag: {other}"))),
        }
    }
}

/// `ClientboundTagQueryPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "tag_query")]
pub struct ClientboundTagQueryPacket {
    #[mc(var)]
    pub transaction_id: i32,
    #[mc(with = nullable_compound)]
    pub tag: Option<CompoundTag>,
}

/// `ServerboundSeenAdvancementsPacket`: `Action` ordinal, then the tab for `OPENED_TAB`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "seen_advancements")]
#[mc(error = "Index {} out of bounds for length 2")]
pub enum ServerboundSeenAdvancementsPacket {
    OpenedTab(Identifier),
    ClosedScreen,
}

/// `ServerboundBlockEntityTagQueryPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "block_entity_tag_query")]
pub struct ServerboundBlockEntityTagQueryPacket {
    #[mc(var)]
    pub transaction_id: i32,
    pub pos: BlockPos,
}

/// `ServerboundEntityTagQueryPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "entity_tag_query")]
pub struct ServerboundEntityTagQueryPacket {
    #[mc(var)]
    pub transaction_id: i32,
    #[mc(var)]
    pub entity_id: i32,
}
