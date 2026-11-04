//! `net.minecraft.server.WorldLoader` + `WorldStem`: the world load pipeline
//! (pack selection -> static tags -> `WORLD` registries -> `DIMENSIONS` -> world data -> reloadable resources).

use mc_world::level_storage::{FeatureFlagSet, WorldDataConfiguration};

use super::packs::{MultiPackResourceManager, PackRepository, PackType, configure_pack_repository};
use super::registry_layer::RegistryLayer;
use super::reloadable_resources::{ReloadError, ReloadableServerResources};
use crate::registry::data_loader::{self, RegistryLoadFailure};
use crate::registry::keys::{DIMENSION_REGISTRIES, WORLD_REGISTRIES};
use crate::registry::{LayeredRegistryAccess, RegistryAccess, build_updated_lookups, load_tags_for_existing_registries};

/// `WorldLoader.PackConfig`.
pub struct PackConfig<'a> {
    pub pack_repository: &'a mut PackRepository,
    pub initial_data_config: WorldDataConfiguration,
    pub safe_mode: bool,
    /// New world (`server.properties` `initial-*-packs`) rather than an existing `level.dat`.
    pub init_mode: bool,
}

impl PackConfig<'_> {
    /// `createResourceManager()`: select the packs and open them. Returns the data configuration to store in `level.dat`.
    pub fn create_resource_manager(&mut self) -> (WorldDataConfiguration, MultiPackResourceManager) {
        let new_pack_config = configure_pack_repository(self.pack_repository, &self.initial_data_config, self.init_mode, self.safe_mode);
        let opened = self.pack_repository.open_all_selected();
        (new_pack_config, MultiPackResourceManager::new(PackType::ServerData, opened))
    }
}

/// `WorldLoader.DataLoadContext`.
pub struct DataLoadContext<'a> {
    pub resources: &'a MultiPackResourceManager,
    pub data_configuration: &'a WorldDataConfiguration,
    /// `datapackWorldRegistries`: static registries (with tags) + the loaded `WORLD` registries.
    pub datapack_world_registries: RegistryAccess,
    /// `datapackDimensions`: the loaded `dimension` registry (level stems from the packs).
    pub datapack_dimensions: RegistryAccess,
}

/// `WorldLoader.DataLoadOutput<D>`.
pub struct DataLoadOutput<D> {
    pub cookie: D,
    /// The final `dimension` registry (the world's own dimensions, baked from `level.dat` / properties).
    pub final_dimensions: RegistryAccess,
}

/// Failure of [`load`].
#[derive(Debug)]
pub enum WorldLoadError {
    Registries(RegistryLoadFailure),
    Reload(ReloadError),
}

impl std::fmt::Display for WorldLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldLoadError::Registries(e) => e.fmt(f),
            WorldLoadError::Reload(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for WorldLoadError {}

impl From<RegistryLoadFailure> for WorldLoadError {
    fn from(e: RegistryLoadFailure) -> Self {
        WorldLoadError::Registries(e)
    }
}

impl From<ReloadError> for WorldLoadError {
    fn from(e: ReloadError) -> Self {
        WorldLoadError::Reload(e)
    }
}

impl WorldLoadError {
    /// What `Main` logs when the load fails (`LOGGER.warn(..., throwable)`).
    pub fn log(&self) {
        log::warn!(
            "Failed to load datapacks, can't proceed with server load. You can either fix your datapacks or reset to vanilla with --safeMode\n{}",
            crate::server::packs::throwable(
                "java.util.concurrent.ExecutionException",
                &format!("net.minecraft.ReportedException: Failed to load registries due to errors{}", match self {
                    WorldLoadError::Registries(e) | WorldLoadError::Reload(ReloadError::Registries(e)) => e.brief(),
                    WorldLoadError::Reload(e) => format!(": {e}"),
                })
            )
        );
    }
}

/// `WorldStem`: the loaded world resources.
pub struct WorldStem<D> {
    pub resource_manager: MultiPackResourceManager,
    pub data_pack_resources: ReloadableServerResources,
    pub registries: LayeredRegistryAccess<RegistryLayer>,
    /// `worldDataAndGenSettings` (whatever the `WorldDataSupplier` returned).
    pub world_data: D,
}

impl<D> WorldStem<D> {
    /// `WorldStem::new` as the `ResultFactory`.
    pub fn new(resource_manager: MultiPackResourceManager, managers: ReloadableServerResources, registries: LayeredRegistryAccess<RegistryLayer>, world_data: D) -> WorldStem<D> {
        WorldStem { resource_manager, data_pack_resources: managers, registries, world_data }
    }

    /// `close()`.
    pub fn close(&self) {
        self.resource_manager.close();
    }
}

/// `WorldLoader.load(config, worldDataSupplier, resultFactory)`.
///
/// Returns the result of `result_factory` and the data configuration chosen by the pack selection (to persist in `level.dat`).
pub fn load<D, R>(
    config: &mut PackConfig,
    world_data_supplier: impl FnOnce(&DataLoadContext) -> DataLoadOutput<D>,
    result_factory: impl FnOnce(MultiPackResourceManager, ReloadableServerResources, LayeredRegistryAccess<RegistryLayer>, D) -> R,
) -> Result<(WorldDataConfiguration, R), WorldLoadError> {
    let (world_data_configuration, resources) = config.create_resource_manager();
    let result = load_with_resources(&world_data_configuration, resources, world_data_supplier, result_factory);
    result.map(|r| (world_data_configuration, r))
}

fn load_with_resources<D, R>(
    world_data_configuration: &WorldDataConfiguration,
    resources: MultiPackResourceManager,
    world_data_supplier: impl FnOnce(&DataLoadContext) -> DataLoadOutput<D>,
    result_factory: impl FnOnce(MultiPackResourceManager, ReloadableServerResources, LayeredRegistryAccess<RegistryLayer>, D) -> R,
) -> Result<R, WorldLoadError> {
    let attempt = (|| -> Result<(ReloadableServerResources, LayeredRegistryAccess<RegistryLayer>, D), WorldLoadError> {
        let initial_layers = RegistryLayer::create_registry_access();
        let static_layer_tags = load_tags_for_existing_registries(&resources, initial_layers.get_layer(RegistryLayer::Static));
        let world_load_context = initial_layers.get_access_for_loading(RegistryLayer::World);
        let world_context_registries = build_updated_lookups(&world_load_context, &static_layer_tags);
        let loaded_world_registries = data_loader::load(&resources, &world_context_registries, WORLD_REGISTRIES)?;
        let dimension_context_registries = RegistryAccess::from_arcs(world_context_registries.registries().chain(loaded_world_registries.registries()).cloned());
        let initial_worldgen_dimensions = data_loader::load(&resources, &dimension_context_registries, DIMENSION_REGISTRIES)?;
        let world_data_and_registries = world_data_supplier(&DataLoadContext {
            resources: &resources,
            data_configuration: world_data_configuration,
            datapack_world_registries: dimension_context_registries,
            datapack_dimensions: initial_worldgen_dimensions,
        });
        let resources_load_context = initial_layers.replace_from(RegistryLayer::World, vec![loaded_world_registries, world_data_and_registries.final_dimensions]);
        let enabled: FeatureFlagSet = world_data_configuration.enabled_features;
        let mut managers = ReloadableServerResources::load_resources(&resources, &resources_load_context, static_layer_tags, enabled)?;
        managers.update_components_and_static_registry_tags();
        let layers = managers.registry_layers().clone();
        Ok((managers, layers, world_data_and_registries.cookie))
    })();
    match attempt {
        Ok((managers, layers, cookie)) => Ok(result_factory(resources, managers, layers, cookie)),
        Err(e) => {
            resources.close();
            Err(e)
        }
    }
}
