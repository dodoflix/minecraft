//! `ClickEvent` and `HoverEvent`.

use crate::component::Component;
use mc_core::ident::Identifier;
use mc_nbt::Tag;

/// `ClickEvent.Action`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickAction {
    OpenUrl,
    OpenFile,
    RunCommand,
    SuggestCommand,
    ShowDialog,
    ChangePage,
    CopyToClipboard,
    Custom,
}

impl ClickAction {
    pub const ALL: [ClickAction; 8] = [
        Self::OpenUrl,
        Self::OpenFile,
        Self::RunCommand,
        Self::SuggestCommand,
        Self::ShowDialog,
        Self::ChangePage,
        Self::CopyToClipboard,
        Self::Custom,
    ];

    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::OpenUrl => "open_url",
            Self::OpenFile => "open_file",
            Self::RunCommand => "run_command",
            Self::SuggestCommand => "suggest_command",
            Self::ShowDialog => "show_dialog",
            Self::ChangePage => "change_page",
            Self::CopyToClipboard => "copy_to_clipboard",
            Self::Custom => "custom",
        }
    }

    /// `isAllowedFromServer`: everything but `open_file` may be (de)serialised.
    pub const fn is_allowed_from_server(self) -> bool {
        !matches!(self, Self::OpenFile)
    }
}

/// `net.minecraft.network.chat.ClickEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum ClickEvent {
    /// Validated like `ExtraCodecs.UNTRUSTED_URI` (absolute `http`/`https` URI); stored verbatim.
    OpenUrl(String),
    /// Client-only; vanilla refuses to (de)serialise it.
    OpenFile(String),
    RunCommand(String),
    SuggestCommand(String),
    /// `Holder<Dialog>`: either a registry id string or an inline dialog compound. Kept opaque.
    ShowDialog(Tag),
    ChangePage(i32),
    CopyToClipboard(String),
    Custom { id: Identifier, payload: Option<Tag> },
}

impl ClickEvent {
    pub fn action(&self) -> ClickAction {
        match self {
            Self::OpenUrl(_) => ClickAction::OpenUrl,
            Self::OpenFile(_) => ClickAction::OpenFile,
            Self::RunCommand(_) => ClickAction::RunCommand,
            Self::SuggestCommand(_) => ClickAction::SuggestCommand,
            Self::ShowDialog(_) => ClickAction::ShowDialog,
            Self::ChangePage(_) => ClickAction::ChangePage,
            Self::CopyToClipboard(_) => ClickAction::CopyToClipboard,
            Self::Custom { .. } => ClickAction::Custom,
        }
    }
}

/// `HoverEvent.Action`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverAction {
    ShowText,
    ShowItem,
    ShowEntity,
}

impl HoverAction {
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::ShowText => "show_text",
            Self::ShowItem => "show_item",
            Self::ShowEntity => "show_entity",
        }
    }
}

/// `ItemStackTemplate` as it appears in `show_item`. The `id` is only syntax-checked here (item
/// registries live in mc-data) and `components` is kept as the opaque serialised patch compound
/// (`None` = empty patch).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStackTemplate {
    pub id: Identifier,
    pub count: i32,
    pub components: Option<Tag>,
}

/// `HoverEvent.EntityTooltipInfo`. `uuid` is the 128 bit value (`most << 64 | least`).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTooltipInfo {
    pub entity_type: Identifier,
    pub uuid: u128,
    pub name: Option<Box<Component>>,
}

/// `net.minecraft.network.chat.HoverEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum HoverEvent {
    ShowText(Box<Component>),
    ShowItem(ItemStackTemplate),
    ShowEntity(EntityTooltipInfo),
}

impl HoverEvent {
    pub fn show_text(value: Component) -> HoverEvent {
        HoverEvent::ShowText(Box::new(value))
    }

    pub fn action(&self) -> HoverAction {
        match self {
            Self::ShowText(_) => HoverAction::ShowText,
            Self::ShowItem(_) => HoverAction::ShowItem,
            Self::ShowEntity(_) => HoverAction::ShowEntity,
        }
    }
}
