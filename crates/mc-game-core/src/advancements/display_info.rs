//! `net.minecraft.advancements.DisplayInfo`.

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{BOOL, Codec};
use mc_protocol::components::{ItemStackTemplate, component_codec};
use mc_text::Component;

use super::advancement_type::AdvancementType;

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    pub icon: ItemStackTemplate,
    pub title: Component,
    pub description: Component,
    /// `ClientAsset.ResourceTexture` (its `id`).
    pub background: Option<Identifier>,
    pub type_: AdvancementType,
    pub show_toast: bool,
    pub announce_to_chat: bool,
    pub hidden: bool,
}

impl DisplayInfo {
    /// `DisplayInfo.CODEC`.
    pub fn codec() -> Codec<DisplayInfo> {
        static C: std::sync::LazyLock<Codec<DisplayInfo>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(DisplayInfo {
                icon: ItemStackTemplate::codec().field_of("icon"),
                title: component_codec().field_of("title"),
                description: component_codec().field_of("description"),
                background: IDENTIFIER.optional_field_of("background"),
                type_: AdvancementType::codec()
                    .optional_field_of_default("frame", AdvancementType::Task),
                show_toast: BOOL.optional_field_of_default("show_toast", true),
                announce_to_chat: BOOL.optional_field_of_default("announce_to_chat", true),
                hidden: BOOL.optional_field_of_default("hidden", false),
            })
            .codec()
        });
        C.clone()
    }

    /// `STREAM_CODEC` (`announceToChat` is not sent).
    pub fn to_network(&self) -> mc_protocol::packets::play::DisplayInfo {
        mc_protocol::packets::play::DisplayInfo {
            icon: self.icon.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            background: self.background.clone(),
            advancement_type: self.type_.into(),
            show_toast: self.show_toast,
            hidden: self.hidden,
        }
    }
}
