//! `net.minecraft.server.packs.repository`: `Pack`, `PackSource`, `PackRepository`, `RepositorySource` and the
//! built-in sources (`ServerPacksSource` for the vanilla + feature packs, `FolderRepositorySource` for `<world>/datapacks`).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mc_text::component::TranslatableArg;
use mc_text::{ChatFormatting, Component};
use mc_world::level_storage::{ContentValidationException, DirectoryValidator, FeatureFlagSet};

use super::pack_format::{InclusiveRange, OverlayMetadataSection, PackFormat, PackMetadataSection};
use super::resources::{
    EmbeddedResourcesSupplier, FileResourcesSupplier, MetadataError, PackResources, PathResourcesSupplier,
};
use super::{GAME_VERSION, PackLocationInfo, PackSelectionConfig, PackType, throwable};

/// `KnownPack` (`namespace:id:version`), what the client and server share for registry data synchronization.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

impl KnownPack {
    pub const VANILLA_NAMESPACE: &'static str = "minecraft";

    /// `KnownPack.vanilla(id)`.
    pub fn vanilla(id: &str) -> KnownPack {
        KnownPack { namespace: "minecraft".to_owned(), id: id.to_owned(), version: GAME_VERSION.to_owned() }
    }

    pub fn is_vanilla(&self) -> bool {
        self.namespace == "minecraft"
    }
}

impl std::fmt::Display for KnownPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.namespace, self.id, self.version)
    }
}

/// `PackSource` (the five built-in instances; vanilla compares them by identity).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PackSource {
    Default,
    BuiltIn,
    Feature,
    World,
    Server,
}

impl PackSource {
    /// `shouldAddAutomatically()`.
    pub fn should_add_automatically(self) -> bool {
        !matches!(self, PackSource::Feature)
    }

    /// `decorate(packDescription)`.
    pub fn decorate(self, pack_description: Component) -> Component {
        let key = match self {
            PackSource::Default => return pack_description,
            PackSource::BuiltIn => "pack.source.builtin",
            PackSource::Feature => "pack.source.feature",
            PackSource::World => "pack.source.world",
            PackSource::Server => "pack.source.server",
        };
        Component::translatable(
            "pack.nameAndSource",
            vec![TranslatableArg::Component(pack_description), TranslatableArg::Component(Component::translatable(key, vec![]))],
        )
        .with_format(ChatFormatting::Gray)
    }
}

/// `PackCompatibility`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PackCompatibility {
    TooOld,
    TooNew,
    Unknown,
    Compatible,
}

impl PackCompatibility {
    pub fn is_compatible(self) -> bool {
        self == PackCompatibility::Compatible
    }

    /// `forVersion(packDeclaredVersions, gameSupportedVersion)`.
    pub fn for_version(declared: InclusiveRange<PackFormat>, game: PackFormat) -> PackCompatibility {
        if declared.min_inclusive.major == i32::MAX {
            PackCompatibility::Unknown
        } else if declared.max_inclusive < game {
            PackCompatibility::TooOld
        } else if game < declared.min_inclusive {
            PackCompatibility::TooNew
        } else {
            PackCompatibility::Compatible
        }
    }

    fn key(self) -> &'static str {
        match self {
            PackCompatibility::TooOld => "old",
            PackCompatibility::TooNew => "new",
            PackCompatibility::Unknown => "unknown",
            PackCompatibility::Compatible => "compatible",
        }
    }

    pub fn description(self) -> Component {
        Component::translatable(format!("pack.incompatible.{}", self.key()), vec![]).with_format(ChatFormatting::Gray)
    }

    pub fn confirmation(self) -> Component {
        Component::translatable(format!("pack.incompatible.confirm.{}", self.key()), vec![])
    }
}

/// `Pack.Position`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PackPosition {
    Top,
    Bottom,
}

impl PackPosition {
    pub fn opposite(self) -> PackPosition {
        match self {
            PackPosition::Top => PackPosition::Bottom,
            PackPosition::Bottom => PackPosition::Top,
        }
    }

    /// `Position.insert(list, value, converter, reverse)`: inserts and returns the index.
    pub fn insert<T>(self, list: &mut Vec<T>, value: T, converter: impl Fn(&T) -> PackSelectionConfig, reverse: bool) -> usize {
        let side = if reverse { self.opposite() } else { self };
        if side == PackPosition::Bottom {
            let mut index = 0;
            while index < list.len() {
                let pack = converter(&list[index]);
                if !pack.fixed_position || pack.default_position != self {
                    break;
                }
                index += 1;
            }
            list.insert(index, value);
            index
        } else {
            let mut index = list.len() as isize - 1;
            while index >= 0 {
                let pack = converter(&list[index as usize]);
                if !pack.fixed_position || pack.default_position != self {
                    break;
                }
                index -= 1;
            }
            list.insert((index + 1) as usize, value);
            (index + 1) as usize
        }
    }
}

/// `Pack.Metadata`.
#[derive(Clone, Debug)]
pub struct PackMetadata {
    pub description: Component,
    pub compatibility: PackCompatibility,
    pub requested_features: FeatureFlagSet,
    pub overlays: Vec<String>,
}

/// `Pack.ResourcesSupplier`.
pub trait ResourcesSupplier: Send + Sync {
    fn open_metadata(&self, location: &PackLocationInfo) -> Arc<dyn PackResources>;
    fn open_resources(&self, location: &PackLocationInfo, metadata: &PackMetadata) -> Vec<Arc<dyn PackResources>>;
}

/// `Pack`.
pub struct Pack {
    location: PackLocationInfo,
    resources: Arc<dyn ResourcesSupplier>,
    metadata: PackMetadata,
    selection_config: PackSelectionConfig,
}

impl std::fmt::Debug for Pack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pack").field("id", &self.location.id).field("metadata", &self.metadata).finish()
    }
}

fn warn_with_throwable(message: &str, class: &str, error: &str) {
    log::warn!("{message}\n{}", throwable(class, error));
}

impl Pack {
    pub fn new(location: PackLocationInfo, resources: Arc<dyn ResourcesSupplier>, metadata: PackMetadata, selection_config: PackSelectionConfig) -> Pack {
        Pack { location, resources, metadata, selection_config }
    }

    /// `readMetaAndCreate`.
    pub fn read_meta_and_create(
        location: PackLocationInfo,
        resources: Arc<dyn ResourcesSupplier>,
        pack_type: PackType,
        selection_config: PackSelectionConfig,
    ) -> Option<Pack> {
        let meta = Pack::read_pack_metadata(&location, resources.as_ref(), pack_type.current_version(), pack_type)?;
        Some(Pack::new(location, resources, meta, selection_config))
    }

    /// `readPackMetadata`, with vanilla's fallback type and log messages.
    pub fn read_pack_metadata(location: &PackLocationInfo, resources: &dyn ResourcesSupplier, current: PackFormat, ty: PackType) -> Option<PackMetadata> {
        let pack = resources.open_metadata(location);
        let result = (|| -> Result<Option<PackMetadata>, MetadataError> {
            let metadata = pack.metadata()?;
            let meta = match metadata.get("pack", |v| PackMetadataSection::decode(v, ty)) {
                Ok(meta) => meta,
                Err(MetadataError::Json(e)) => {
                    warn_with_throwable("Error reading pack metadata, attempting fallback type", "com.google.gson.JsonParseException", &e);
                    pack.metadata()?.get("pack", PackMetadataSection::decode_fallback)?
                }
                Err(e) => return Err(e),
            };
            let Some(meta) = meta else {
                log::warn!("Missing metadata in pack {}", location.id);
                return Ok(None);
            };
            let metadata = pack.metadata()?;
            let required_flags = metadata.get("features", super::pack_format::decode_feature_flags_section)?.unwrap_or(FeatureFlagSet::EMPTY);
            let compatibility = PackCompatibility::for_version(meta.supported_formats, current);
            let overlays = metadata.get("overlays", |v| OverlayMetadataSection::decode(v, ty))?;
            let overlay_set = overlays.map(|o| o.overlays_for_version(current)).unwrap_or_default();
            Ok(Some(PackMetadata { description: meta.description, compatibility, requested_features: required_flags, overlays: overlay_set }))
        })();
        pack.close();
        match result {
            Ok(meta) => meta,
            Err(e) => {
                let class = match e {
                    MetadataError::Json(_) => "com.google.gson.JsonParseException",
                    MetadataError::Io(_) => "java.io.IOException",
                };
                warn_with_throwable(&format!("Failed to read pack {} metadata", location.id), class, &e.to_string());
                None
            }
        }
    }

    pub fn location(&self) -> &PackLocationInfo {
        &self.location
    }

    pub fn title(&self) -> &Component {
        &self.location.title
    }

    pub fn description(&self) -> &Component {
        &self.metadata.description
    }

    pub fn chat_link(&self, enabled: bool) -> Component {
        self.location.create_chat_link(enabled, &self.metadata.description)
    }

    pub fn compatibility(&self) -> PackCompatibility {
        self.metadata.compatibility
    }

    pub fn requested_features(&self) -> FeatureFlagSet {
        self.metadata.requested_features
    }

    pub fn metadata(&self) -> &PackMetadata {
        &self.metadata
    }

    /// `open()`: the pack's resource layers (one, or an overlayed pack).
    pub fn open(&self) -> Vec<Arc<dyn PackResources>> {
        self.resources.open_resources(&self.location, &self.metadata)
    }

    pub fn id(&self) -> &str {
        &self.location.id
    }

    pub fn selection_config(&self) -> PackSelectionConfig {
        self.selection_config
    }

    pub fn is_required(&self) -> bool {
        self.selection_config.required
    }

    pub fn is_fixed_position(&self) -> bool {
        self.selection_config.fixed_position
    }

    pub fn default_position(&self) -> PackPosition {
        self.selection_config.default_position
    }

    pub fn pack_source(&self) -> PackSource {
        self.location.source
    }
}

impl PartialEq for Pack {
    fn eq(&self, other: &Pack) -> bool {
        self.location.id == other.location.id && self.location.source == other.location.source && self.location.known_pack_info == other.location.known_pack_info
    }
}

// ---- repository sources ------------------------------------------------------------------------------------------

/// `RepositorySource`.
pub trait RepositorySource: Send + Sync {
    fn load_packs(&self, result: &mut dyn FnMut(Pack));
}

/// `PackDetector` + `FolderRepositorySource.FolderPackDetector`.
fn detect_pack_resources(content: &Path, validator: &DirectoryValidator, issues: &mut Vec<mc_world::level_storage::ForbiddenSymlinkInfo>) -> io::Result<Option<Arc<dyn ResourcesSupplier>>> {
    let mut target = content.to_path_buf();
    let mut attributes = match fs::symlink_metadata(content) {
        Ok(a) => a,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if attributes.file_type().is_symlink() {
        validator.validate_symlink_into(content, issues)?;
        if !issues.is_empty() {
            return Ok(None);
        }
        target = fs::read_link(content)?;
        attributes = fs::symlink_metadata(&target)?;
    }
    if attributes.is_dir() {
        validator.validate_known_directory(&target, issues)?;
        if !issues.is_empty() {
            return Ok(None);
        }
        Ok(if target.join("pack.mcmeta").is_file() { Some(Arc::new(PathResourcesSupplier { content: target })) } else { None })
    } else if attributes.is_file() && target.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".zip")) {
        Ok(Some(Arc::new(FileResourcesSupplier { content: target })))
    } else {
        Ok(None)
    }
}

/// `FolderRepositorySource`.
pub struct FolderRepositorySource {
    folder: PathBuf,
    pack_type: PackType,
    pack_source: PackSource,
    validator: DirectoryValidator,
}

const DISCOVERED_PACK_SELECTION_CONFIG: PackSelectionConfig = PackSelectionConfig { required: false, default_position: PackPosition::Top, fixed_position: false };

impl FolderRepositorySource {
    pub fn new(folder: PathBuf, pack_type: PackType, pack_source: PackSource, validator: DirectoryValidator) -> FolderRepositorySource {
        FolderRepositorySource { folder, pack_type, pack_source, validator }
    }

    /// `discoverPacks(folder, validator, result)`.
    pub fn discover_packs(folder: &Path, validator: &DirectoryValidator, result: &mut dyn FnMut(&Path, Arc<dyn ResourcesSupplier>)) -> io::Result<()> {
        for entry in fs::read_dir(folder)? {
            let content = entry?.path();
            let mut issues = Vec::new();
            match detect_pack_resources(&content, validator, &mut issues) {
                Ok(_) if !issues.is_empty() => log::warn!("Ignoring potential pack entry: {}", ContentValidationException::message(&content, &issues)),
                Ok(Some(resources)) => result(&content, resources),
                Ok(None) => log::info!("Found non-pack entry '{}', ignoring", content.display()),
                Err(e) => warn_with_throwable(&format!("Failed to read properties of '{}', ignoring", content.display()), "java.io.IOException", &e.to_string()),
            }
        }
        Ok(())
    }
}

impl RepositorySource for FolderRepositorySource {
    fn load_packs(&self, result: &mut dyn FnMut(Pack)) {
        let discovered = (|| {
            fs::create_dir_all(&self.folder)?;
            FolderRepositorySource::discover_packs(&self.folder, &self.validator, &mut |content, resources| {
                let name = content.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let info = PackLocationInfo { id: format!("file/{name}"), title: Component::literal(name), source: self.pack_source, known_pack_info: None };
                if let Some(pack) = Pack::read_meta_and_create(info, resources, self.pack_type, DISCOVERED_PACK_SELECTION_CONFIG) {
                    result(pack);
                }
            })
        })();
        if let Err(e) = discovered {
            warn_with_throwable(&format!("Failed to list packs in {}", self.folder.display()), "java.io.IOException", &e.to_string());
        }
    }
}

const VANILLA_SELECTION_CONFIG: PackSelectionConfig = PackSelectionConfig { required: false, default_position: PackPosition::Bottom, fixed_position: false };
const FEATURE_SELECTION_CONFIG: PackSelectionConfig = PackSelectionConfig { required: false, default_position: PackPosition::Top, fixed_position: false };

/// Directory of the built-in (feature) packs inside the vanilla pack: `data/minecraft/datapacks`.
const FEATURE_PACKS_DIR: &str = "data/minecraft/datapacks/";

/// `ServerPacksSource` (+ `BuiltInPackSource`): the `vanilla` pack and the feature packs bundled in the server jar.
pub struct ServerPacksSource;

impl ServerPacksSource {
    /// `VANILLA_PACK_INFO`.
    pub fn vanilla_pack_info() -> PackLocationInfo {
        PackLocationInfo {
            id: "vanilla".to_owned(),
            title: Component::translatable("dataPack.vanilla.name", vec![]),
            source: PackSource::BuiltIn,
            known_pack_info: Some(KnownPack::vanilla("core")),
        }
    }

    /// `BUILT_IN_METADATA`: `PackMetadataSection(dataPack.vanilla.description, 121.0..121.*)` and the default feature flags.
    fn built_in_metadata() -> serde_json::Map<String, serde_json::Value> {
        let serde_json::Value::Object(map) = serde_json::json!({
            "pack": {"description": {"translate": "dataPack.vanilla.description"}, "min_format": 121, "max_format": 121},
            "features": {"enabled": ["minecraft:vanilla"]},
        }) else {
            unreachable!()
        };
        map
    }

    fn vanilla_pack() -> Option<Pack> {
        let supplier = Arc::new(EmbeddedResourcesSupplier {
            root: String::new(),
            exposed_namespaces: Some(HashSet::from(["minecraft".to_owned()])),
            built_in_metadata: Some(ServerPacksSource::built_in_metadata()),
        });
        Pack::read_meta_and_create(ServerPacksSource::vanilla_pack_info(), supplier, PackType::ServerData, VANILLA_SELECTION_CONFIG)
    }

    /// `listBundledPacks`: every directory of `data/minecraft/datapacks/` of the embedded pack that holds a `pack.mcmeta`.
    fn feature_packs() -> Vec<Pack> {
        let ids: std::collections::BTreeSet<&str> = mc_data::datapack::paths()
            .filter_map(|p| p.strip_prefix(FEATURE_PACKS_DIR))
            .filter_map(|rest| rest.strip_suffix("/pack.mcmeta"))
            .filter(|id| !id.contains('/'))
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                let info = PackLocationInfo { id: id.to_owned(), title: Component::literal(id), source: PackSource::Feature, known_pack_info: Some(KnownPack::vanilla(id)) };
                let supplier = Arc::new(EmbeddedResourcesSupplier { root: format!("{FEATURE_PACKS_DIR}{id}/"), exposed_namespaces: None, built_in_metadata: None });
                Pack::read_meta_and_create(info, supplier, PackType::ServerData, FEATURE_SELECTION_CONFIG)
            })
            .collect()
    }

    /// `ServerPacksSource.createPackRepository(datapackDir, validator)`.
    pub fn create_pack_repository(datapack_dir: PathBuf, validator: DirectoryValidator) -> PackRepository {
        PackRepository::new(vec![
            Box::new(ServerPacksSource),
            Box::new(FolderRepositorySource::new(datapack_dir, PackType::ServerData, PackSource::World, validator)),
        ])
    }

    /// `createVanillaTrustedRepository()`.
    pub fn create_vanilla_trusted_repository() -> PackRepository {
        PackRepository::new(vec![Box::new(ServerPacksSource)])
    }
}

impl RepositorySource for ServerPacksSource {
    fn load_packs(&self, result: &mut dyn FnMut(Pack)) {
        if let Some(vanilla) = ServerPacksSource::vanilla_pack() {
            result(vanilla);
        }
        for pack in ServerPacksSource::feature_packs() {
            result(pack);
        }
    }
}

// ---- PackRepository ----------------------------------------------------------------------------------------------

/// `PackRepository`: all discoverable packs (sorted by id) and the selected ones, lowest priority first.
pub struct PackRepository {
    sources: Vec<Box<dyn RepositorySource>>,
    available: BTreeMap<String, Arc<Pack>>,
    selected: Vec<Arc<Pack>>,
}

impl PackRepository {
    pub fn new(sources: Vec<Box<dyn RepositorySource>>) -> PackRepository {
        PackRepository { sources, available: BTreeMap::new(), selected: Vec::new() }
    }

    /// `displayPackList`.
    pub fn display_pack_list<'a>(packs: impl IntoIterator<Item = &'a Arc<Pack>>) -> String {
        packs
            .into_iter()
            .map(|p| format!("{}{}", p.id(), if p.compatibility().is_compatible() { "" } else { " (incompatible)" }))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `reload()`.
    pub fn reload(&mut self) {
        let currently_selected: Vec<String> = self.selected.iter().map(|p| p.id().to_owned()).collect();
        self.available = self.discover_available();
        self.selected = self.rebuild_selected(&currently_selected);
    }

    fn discover_available(&self) -> BTreeMap<String, Arc<Pack>> {
        let mut discovered = BTreeMap::new();
        for source in &self.sources {
            source.load_packs(&mut |pack| {
                discovered.insert(pack.id().to_owned(), Arc::new(pack));
            });
        }
        discovered
    }

    /// `isAbleToClearAnyPack()`.
    pub fn is_able_to_clear_any_pack(&self) -> bool {
        let new_selected = self.rebuild_selected::<&str>(&[]);
        !(self.selected.len() == new_selected.len() && self.selected.iter().zip(&new_selected).all(|(a, b)| a == b))
    }

    /// `setSelected(packs)`.
    pub fn set_selected<S: AsRef<str>>(&mut self, packs: impl IntoIterator<Item = S>) {
        let names: Vec<String> = packs.into_iter().map(|s| s.as_ref().to_owned()).collect();
        self.selected = self.rebuild_selected(&names);
    }

    /// `addPack(id)`.
    pub fn add_pack(&mut self, pack_id: &str) -> bool {
        match self.available.get(pack_id) {
            Some(pack) if !self.selected.iter().any(|s| s == pack) => {
                self.selected.push(pack.clone());
                true
            }
            _ => false,
        }
    }

    /// `removePack(id)`.
    pub fn remove_pack(&mut self, pack_id: &str) -> bool {
        match self.available.get(pack_id) {
            Some(pack) if self.selected.iter().any(|s| s == pack) => {
                self.selected.retain(|s| s != pack);
                true
            }
            _ => false,
        }
    }

    fn rebuild_selected<S: AsRef<str>>(&self, selected_names: &[S]) -> Vec<Arc<Pack>> {
        let mut selected_and_present: Vec<Arc<Pack>> = selected_names.iter().filter_map(|id| self.available.get(id.as_ref()).cloned()).collect();
        for pack in self.available.values() {
            if pack.is_required() && !selected_and_present.iter().any(|p| p == pack) {
                pack.default_position().insert(&mut selected_and_present, pack.clone(), |p| p.selection_config(), false);
            }
        }
        selected_and_present
    }

    pub fn available_ids(&self) -> impl Iterator<Item = &str> {
        self.available.keys().map(String::as_str)
    }

    pub fn available_packs(&self) -> impl Iterator<Item = &Arc<Pack>> {
        self.available.values()
    }

    /// `getSelectedIds()` (a `LinkedHashSet`: duplicates collapse, order kept).
    pub fn selected_ids(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.selected.iter().map(|p| p.id()).filter(|id| seen.insert(*id)).map(str::to_owned).collect()
    }

    /// `getRequestedFeatureFlags()`.
    pub fn requested_feature_flags(&self) -> FeatureFlagSet {
        self.selected.iter().map(|p| p.requested_features()).fold(FeatureFlagSet::EMPTY, FeatureFlagSet::join)
    }

    pub fn selected_packs(&self) -> &[Arc<Pack>] {
        &self.selected
    }

    pub fn get_pack(&self, id: &str) -> Option<&Arc<Pack>> {
        self.available.get(id)
    }

    pub fn is_available(&self, id: &str) -> bool {
        self.available.contains_key(id)
    }

    /// `openAllSelected()`.
    pub fn open_all_selected(&self) -> Vec<Arc<dyn PackResources>> {
        self.selected.iter().flat_map(|p| p.open()).collect()
    }
}
