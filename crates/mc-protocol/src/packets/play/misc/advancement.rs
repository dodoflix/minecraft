//! `ClientboundUpdateAdvancementsPacket` and the network form of advancements (rewards and criteria triggers are
//! data-pack only and never sent; the client learns the criteria names from `requirements`).
use mc_core::ident::Identifier;
use mc_text::Component;

use crate::codec::DecodeResult;
use crate::codec::Instant;
use crate::components::ItemStackTemplate;
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

id_enum!(
    /// `AdvancementType` (`STREAM_CODEC`: VarInt id, unknown = `TASK`)
    AdvancementType { Task = 0, Challenge = 1, Goal = 2 } out_of_bounds = ZERO
);

/// `AdvancementRequirements.STREAM_CODEC`: a list of criteria-name lists.
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct AdvancementRequirements {
    pub requirements: Vec<Vec<String>>,
}

/// `DisplayInfo.STREAM_CODEC`. `announce_to_chat` is not sent (the client always reads `false`), so it is not modeled.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    pub icon: ItemStackTemplate,
    pub title: Component,
    pub description: Component,
    /// `ClientAsset.ResourceTexture` id of the root background.
    pub background: Option<Identifier>,
    pub advancement_type: AdvancementType,
    pub show_toast: bool,
    pub hidden: bool,
}

impl Encode for DisplayInfo {
    fn encode(&self, out: &mut Vec<u8>) {
        self.title.encode(out);
        self.description.encode(out);
        self.icon.encode(out);
        self.advancement_type.encode(out);
        let flags = self.background.is_some() as i32
            | (self.show_toast as i32) << 1
            | (self.hidden as i32) << 2;
        flags.encode(out);
        if let Some(background) = &self.background {
            background.encode(out);
        }
    }
}
impl Decode for DisplayInfo {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let title = Component::decode(input)?;
        let description = Component::decode(input)?;
        let icon = ItemStackTemplate::decode(input)?;
        let advancement_type = AdvancementType::decode(input)?;
        let flags = i32::decode(input)?;
        let background = if flags & 1 != 0 {
            Some(Identifier::decode(input)?)
        } else {
            None
        };
        Ok(Self {
            icon,
            title,
            description,
            background,
            advancement_type,
            show_toast: flags & 2 != 0,
            hidden: flags & 4 != 0,
        })
    }
}

/// `Advancement.STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Advancement {
    pub parent: Option<Identifier>,
    pub display: Option<DisplayInfo>,
    pub requirements: AdvancementRequirements,
    pub sends_telemetry_event: bool,
}

/// `AdvancementHolder.STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AdvancementHolder {
    pub id: Identifier,
    pub value: Advancement,
}

/// `CriterionProgress.STREAM_CODEC`: the time the criterion was obtained, if it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct CriterionProgress {
    pub obtained: Option<Instant>,
}

/// `AdvancementProgress.STREAM_CODEC`: criterion name -> progress, in the map's iteration order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct AdvancementProgress {
    pub criteria: Vec<(String, CriterionProgress)>,
}

/// `ClientboundUpdateAdvancementsPacket.PositionedAdvancement`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PositionedAdvancement {
    pub advancement: AdvancementHolder,
    pub x: f32,
    pub y: f32,
}

/// `ClientboundUpdateAdvancementsPacket`: `removed` is a set and `progress` a map in vanilla; both keep their wire order.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "update_advancements")]
pub struct ClientboundUpdateAdvancementsPacket {
    pub should_reset: bool,
    pub added: Vec<PositionedAdvancement>,
    pub removed: Vec<Identifier>,
    pub progress: Vec<(Identifier, AdvancementProgress)>,
    pub show_advancements: bool,
}
