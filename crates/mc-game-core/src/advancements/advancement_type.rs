//! `net.minecraft.advancements.AdvancementType`.

use mc_dfu::{Codec, string_enum_codec};
use mc_text::{ChatFormatting, Component, TranslatableArg};

use super::advancement::AdvancementHolder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvancementType {
    Task,
    Challenge,
    Goal,
}

impl AdvancementType {
    const VALUES: &'static [(AdvancementType, &'static str)] = &[
        (AdvancementType::Task, "task"),
        (AdvancementType::Challenge, "challenge"),
        (AdvancementType::Goal, "goal"),
    ];

    /// `getSerializedName()`.
    pub fn name(self) -> &'static str {
        Self::VALUES[self.id() as usize].1
    }

    pub fn id(self) -> i32 {
        self as i32
    }

    /// `AdvancementType.CODEC` (`StringRepresentable.fromEnum`).
    pub fn codec() -> Codec<AdvancementType> {
        string_enum_codec(Self::VALUES)
    }

    /// `getChatColor()`.
    pub fn chat_color(self) -> ChatFormatting {
        match self {
            AdvancementType::Task | AdvancementType::Goal => ChatFormatting::Green,
            AdvancementType::Challenge => ChatFormatting::DarkPurple,
        }
    }

    /// `getDisplayName()`.
    pub fn display_name(self) -> Component {
        Component::translatable(format!("advancements.toast.{}", self.name()), vec![])
    }

    /// `createAnnouncement(holder, player)`: `player_name` is `player.getDisplayName()`.
    pub fn create_announcement(
        self,
        holder: &AdvancementHolder,
        player_name: Component,
    ) -> Component {
        Component::translatable(
            format!("chat.type.advancement.{}", self.name()),
            vec![
                TranslatableArg::Component(player_name),
                TranslatableArg::Component(super::advancement::name(holder)),
            ],
        )
    }
}

impl From<AdvancementType> for mc_protocol::packets::play::AdvancementType {
    fn from(t: AdvancementType) -> Self {
        match t {
            AdvancementType::Task => Self::Task,
            AdvancementType::Challenge => Self::Challenge,
            AdvancementType::Goal => Self::Goal,
        }
    }
}
