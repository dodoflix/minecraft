//! `net.minecraft.server.packs`: pack repository (`repository`), pack contents (`resources`), the layered resource
//! manager (`resource_manager`), `pack.mcmeta` sections (`pack_format`) and the server-side pack selection
//! (`selection` = `MinecraftServer.configurePackRepository`).

pub mod pack_format;
pub mod repository;
pub mod resource_manager;
pub mod resources;
pub mod selection;

use mc_text::Component;

pub use pack_format::{InclusiveRange, OverlayMetadataSection, PackFormat, PackMetadataSection, ResourceFilterSection};
pub use repository::{
    FolderRepositorySource, KnownPack, Pack, PackCompatibility, PackMetadata, PackPosition, PackRepository, PackSource, RepositorySource, ServerPacksSource,
};
pub use resource_manager::{FileToIdConverter, MultiPackResourceManager, Resource, ResourceManager};
pub use resources::{IoSupplier, PackResources, ResourceMetadata};
pub use selection::{configure_pack_repository, get_selected_packs};

/// `SharedConstants.getCurrentVersion().id()`.
pub const GAME_VERSION: &str = "26.3";
/// `SharedConstants.getCurrentVersion().packVersion(PackType.SERVER_DATA)` (`data_major`/`data_minor` of `version.json`).
pub const DATA_PACK_FORMAT: PackFormat = PackFormat::of(121);
/// `SharedConstants.getCurrentVersion().packVersion(PackType.CLIENT_RESOURCES)`.
pub const RESOURCE_PACK_FORMAT: PackFormat = PackFormat::of_minor(97, 1);

/// `PackType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PackType {
    ClientResources,
    ServerData,
}

impl PackType {
    /// `getDirectory()`.
    pub const fn directory(self) -> &'static str {
        match self {
            PackType::ClientResources => "assets",
            PackType::ServerData => "data",
        }
    }

    /// `SharedConstants.getCurrentVersion().packVersion(type)`.
    pub const fn current_version(self) -> PackFormat {
        match self {
            PackType::ClientResources => RESOURCE_PACK_FORMAT,
            PackType::ServerData => DATA_PACK_FORMAT,
        }
    }
}

/// `PackLocationInfo`.
#[derive(Clone, Debug)]
pub struct PackLocationInfo {
    pub id: String,
    pub title: Component,
    pub source: PackSource,
    pub known_pack_info: Option<KnownPack>,
}

impl PackLocationInfo {
    /// `createChatLink(enabled, description)`.
    pub fn create_chat_link(&self, enabled: bool, description: &Component) -> Component {
        use mc_text::{ChatFormatting, HoverEvent};
        let hover = Component::empty().append(self.title.clone()).append_str("\n").append(description.clone());
        mc_text::util::wrap_in_square_brackets(self.source.decorate(Component::literal(self.id.clone()))).with_style_fn(|s| {
            s.with_color_formatting(Some(if enabled { ChatFormatting::Green } else { ChatFormatting::Red }))
                .with_insertion(Some(mc_brigadier::arguments::escape_if_required(&self.id)))
                .with_hover_event(Some(HoverEvent::show_text(hover)))
        })
    }
}

/// `PackSelectionConfig`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PackSelectionConfig {
    pub required: bool,
    pub default_position: PackPosition,
    pub fixed_position: bool,
}

/// `Throwable` as Log4j prints it without stack frames: `class: message`.
pub(crate) fn throwable(class: &str, message: &str) -> String {
    format!("{class}: {message}")
}
