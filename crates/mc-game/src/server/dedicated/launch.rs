//! `net.minecraft.server.Main` after the option/properties/EULA handling (done by the `mc-server` binary):
//! world storage access, level data, the data pack repository, the world load and the start of the server thread
//! (`MinecraftServer.spin`).

use super::console::ConsoleInputQueue;
use super::dedicated_server::DedicatedServer;
use super::options::ServerOptions;
use super::properties::{DedicatedServerProperties, DedicatedServerSettings};
use super::stop::ServerStopSignal;
use crate::crash_report::install_panic_hook;
use crate::registry::{Registry, keys};
use crate::server::level_load_listener::LoggingLevelLoadListener;
use crate::server::minecraft_server::{ReloadableResources, ServerInit, SERVER_THREAD_NAME};
use crate::server::notifications::NotificationManager;
use crate::server::packs::ServerPacksSource;
use crate::server::players::name_resolver::{CachedUserNameToIdResolver, NoProfileLookup};
use crate::server::shared::ServerShared;
use crate::server::world_dimensions::WorldDimensions;
use crate::server::world_loader::{DataLoadContext, DataLoadOutput, PackConfig, WorldLoadError, WorldStem, load};
use crate::server::MinecraftServer;
use mc_nbt::CompoundTag;
use mc_world::level_storage::{
    DifficultySettings, FeatureFlagSet, GameType, LevelResource, LevelSettings, LevelStorageSource, PrimaryLevelData, WorldDataConfiguration,
    WorldGenSettings, WorldOptions, get_data_version, requires_file_fixing,
};
use mc_worldgen::biome::BiomeRegistry;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// `MinecraftServer.DEMO_SETTINGS` (demo world: survival, default difficulty, `WorldOptions.DEMO_OPTIONS`).
fn demo_settings(data_configuration: WorldDataConfiguration) -> LevelSettings {
    LevelSettings {
        level_name: "Demo World".to_owned(),
        game_type: GameType::Survival,
        difficulty_settings: DifficultySettings::DEFAULT,
        allow_commands: false,
        data_configuration,
    }
}

/// `WorldOptions.DEMO_OPTIONS`: the seed of the demo world (`"North Carolina".hashCode()`).
fn demo_world_options() -> WorldOptions {
    WorldOptions::new(i64::from(mc_core::math::java::java_string_hash("North Carolina")), true, true)
}

/// `LOGGER.error(FATAL_MARKER, "Failed to start the minecraft server", e)`.
fn fatal(error: impl std::fmt::Display) {
    log::error!("Failed to start the minecraft server\n{error}");
}

/// The `WorldDataAndGenSettings` of `LevelDataAndDimensions`.
pub struct WorldDataAndGenSettings {
    pub data: PrimaryLevelData,
    pub gen_settings: WorldGenSettings,
}

/// `Main.main` from the `Services.create` line on, plus `MinecraftServer.spin` and the shutdown hook. Returns the
/// process exit code (vanilla's `Main.main` returns normally on every failure: 0).
pub fn launch(options: ServerOptions, settings: DedicatedServerSettings) -> i32 {
    match start(options, settings) {
        Ok(code) => code,
        Err(message) => {
            fatal(message);
            0
        }
    }
}

fn start(options: ServerOptions, settings: DedicatedServerSettings) -> Result<i32, String> {
    install_panic_hook();
    let universe = PathBuf::from(&options.universe);
    let name_to_id_cache = Arc::new(CachedUserNameToIdResolver::new(Arc::new(NoProfileLookup), universe.join("usercache.json")));
    let notification_manager = Arc::new(NotificationManager::new());
    let properties = settings.get_properties().clone();
    let level_name = options.world_name.clone().unwrap_or_else(|| properties.level_name.clone());
    let level_storage_source = LevelStorageSource::create_default(&universe).map_err(|e| e.to_string())?;
    let access = level_storage_source.validate_and_create_access(&level_name).map_err(|e| e.to_string())?;

    let level_data_tag: Option<CompoundTag> = if access.has_world_data() {
        let unfixed = match access.get_unfixed_data_tag_with_fallback() {
            Ok(tag) => tag,
            Err(e) => {
                log::error!("Failed to load world data. World files may be corrupted. Shutting down.\n{e}");
                return Ok(0);
            }
        };
        let summary = match access.fix_and_get_summary_from_tag(&unfixed) {
            Ok(summary) => summary,
            Err(e) => {
                log::error!("Failed to load world data. World files may be corrupted. Shutting down.\n{e}");
                return Ok(0);
            }
        };
        if summary.requires_manual_conversion {
            log::info!("This world must be opened in an older version (like 1.6.4) to be safely converted");
            return Ok(0);
        }
        if !summary.is_compatible() {
            log::info!("This world was created by an incompatible version.");
            return Ok(0);
        }
        if requires_file_fixing(get_data_version(&unfixed)) {
            // `DataFixers.getFileFixer().fix(..)`: the file fixer steps are not ported.
            return Err("This world was saved by an older version and must be upgraded first: the file fixer is not available".to_owned());
        }
        Some(unfixed)
    } else {
        None
    };

    let safe_mode = options.safe_mode;
    if safe_mode {
        log::warn!("Safe mode active, only vanilla datapack will be loaded");
    }

    let mut pack_repository =
        ServerPacksSource::create_pack_repository(access.get_level_path(LevelResource::DATAPACK_DIR), level_storage_source.get_world_dir_validator().clone());

    let (init_mode, data_config_to_use) = match &level_data_tag {
        Some(tag) => (false, LevelStorageSource::read_data_config(tag)),
        None => (true, WorldDataConfiguration { data_packs: Some(properties.initial_data_pack_configuration.clone()), enabled_features: FeatureFlagSet::DEFAULT_FLAGS }),
    };
    let mut pack_config = PackConfig { pack_repository: &mut pack_repository, initial_data_config: data_config_to_use, safe_mode, init_mode };
    let loaded = load(
        &mut pack_config,
        |context| world_data_supplier(context, &access, level_data_tag.as_ref(), &properties, &options),
        WorldStem::new,
    );
    let (_selected_config, world_stem) = match loaded {
        Ok(loaded) => loaded,
        Err(e) => {
            log_load_error(&e);
            return Ok(0);
        }
    };

    if options.force_upgrade || options.recreate_region_files {
        log::info!("Forcing world upgrade!");
        log::error!("The world upgrader is not available: continuing without upgrading the world");
    }

    let WorldStem { resource_manager, data_pack_resources, registries, world_data } = world_stem;
    let WorldDataAndGenSettings { data, gen_settings } = world_data;
    if let Err(e) = access.save_data_tag(&data, None) {
        return Err(format!("Failed to save the level data: {e}"));
    }
    let player_data_storage = access.create_player_storage().map_err(|e| e.to_string())?;

    let console_input = ConsoleInputQueue::new();
    let stop_signal = ServerStopSignal::new();
    if let Err(e) = stop_signal.install_signal_handlers() {
        log::warn!("Failed to install the signal handlers: {e}");
    }
    let dedicated = DedicatedServer::new(settings, console_input, stop_signal.clone()).map_err(|e| e.to_string())?;

    let (shared, tasks) = ServerShared::new();
    let init = ServerInit {
        server_directory: PathBuf::new(),
        storage_source: access,
        player_data_storage,
        world_data: data,
        world_gen_settings: gen_settings,
        pack_repository,
        resources: ReloadableResources { resource_manager, managers: data_pack_resources },
        registries,
        dedicated,
        notification_manager,
        name_to_id_cache,
        level_load_listener: Box::new(LoggingLevelLoadListener::for_dedicated_server()),
    };
    let (port, demo, server_id) = (options.port, options.demo, options.server_id.clone());
    let server_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name(SERVER_THREAD_NAME.to_owned())
        .spawn(move || {
            install_panic_hook();
            match MinecraftServer::new(server_shared, tasks, init) {
                Ok(mut server) => {
                    server.port = port;
                    server.is_demo = demo;
                    server.server_id = server_id;
                    server.run_server();
                }
                Err(message) => fatal(format!("java.lang.IllegalStateException: {message}")),
            }
        })
        .map_err(|e| e.to_string())?;

    // The shutdown hook: SIGINT/SIGTERM halt the server and wait for it (`halt(true)`).
    while !thread.is_finished() {
        if stop_signal.is_stop_requested() {
            shared.halt();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = thread.join();
    Ok(0)
}

fn log_load_error(error: &WorldLoadError) {
    error.log();
}

/// The `WorldDataSupplier` lambda of `Main`: the world data of an existing world, or `createNewWorldData`.
fn world_data_supplier(
    context: &DataLoadContext,
    access: &mc_world::level_storage::LevelStorageAccess,
    level_data_tag: Option<&CompoundTag>,
    properties: &DedicatedServerProperties,
    options: &ServerOptions,
) -> DataLoadOutput<WorldDataAndGenSettings> {
    let datapack_dimensions: Option<&Registry> = context.datapack_dimensions.registry(keys::LEVEL_STEM).map(|r| r.as_ref());
    let (data, gen_settings, dimensions) = match level_data_tag {
        Some(tag) => {
            let result = LevelStorageSource::get_level_data_and_dimensions(access, tag, context.data_configuration.clone())
                .unwrap_or_else(|e| panic!("{e}"));
            let gen_settings = match result.world_gen_settings {
                Ok(settings) => settings,
                Err(e) => {
                    log::error!("Unable to read or access the world gen settings file!\n{e}");
                    let dims = create_normal_world_dimensions(context);
                    WorldGenSettings { options: random_world_options(), dimensions: dims.to_stored() }
                }
            };
            let dimensions = WorldDimensions::from_stored(&gen_settings.dimensions).unwrap_or_else(|e| panic!("{e}"));
            (result.world_data, gen_settings, dimensions)
        }
        None => {
            log::info!("No existing world data, creating new world");
            create_new_world_data(context, properties, options.demo, options.bonus_chest)
        }
    };
    let (final_dimensions, special) = dimensions.bake(datapack_dimensions).unwrap_or_else(|e| panic!("{e}"));
    let mut data = data;
    data.special_world_property = special;
    DataLoadOutput { cookie: WorldDataAndGenSettings { data, gen_settings }, final_dimensions }
}

fn random_world_options() -> WorldOptions {
    use rand::Rng;
    WorldOptions::new(rand::rng().random::<i64>(), true, false)
}

/// `WorldPresets.createNormalWorldDimensions`.
fn create_normal_world_dimensions(context: &DataLoadContext) -> WorldDimensions {
    preset_dimensions(context, "minecraft:normal").unwrap_or_else(|| panic!("Invalid datapack contents: can't find default preset"))
}

fn preset_dimensions(context: &DataLoadContext, id: &str) -> Option<WorldDimensions> {
    let id = mc_core::ident::Identifier::try_parse(id)?;
    let registry = context.datapack_world_registries.registry(keys::WORLD_PRESET)?;
    let entry = registry.entry_by_key(&id)?;
    WorldDimensions::from_preset_json(&entry.json).ok()
}

/// `Main.createNewWorldData`.
fn create_new_world_data(
    context: &DataLoadContext,
    properties: &DedicatedServerProperties,
    demo_mode: bool,
    bonus_chest: bool,
) -> (PrimaryLevelData, WorldGenSettings, WorldDimensions) {
    let (level_settings, world_options, dimensions) = if demo_mode {
        (demo_settings(context.data_configuration.clone()), demo_world_options(), create_normal_world_dimensions(context))
    } else {
        let level_settings = LevelSettings {
            level_name: properties.level_name.clone(),
            game_type: *properties.game_mode.get(),
            difficulty_settings: DifficultySettings { difficulty: *properties.difficulty.get(), hardcore: properties.hardcore, locked: false },
            allow_commands: false,
            data_configuration: context.data_configuration.clone(),
        };
        let mut world_options = properties.world_options.clone();
        if bonus_chest {
            world_options.generate_bonus_chest = true;
        }
        (level_settings, world_options, create_dimensions(context, properties))
    };
    let special = dimensions.special_world_property();
    let data = PrimaryLevelData::new(level_settings, special);
    let gen_settings = WorldGenSettings { options: world_options, dimensions: dimensions.to_stored() };
    (data, gen_settings, dimensions)
}

/// `DedicatedServerProperties.WorldDimensionData.create`: the `level-type` preset; for the flat preset the overworld
/// generator is replaced by one built from `generator-settings`.
fn create_dimensions(context: &DataLoadContext, properties: &DedicatedServerProperties) -> WorldDimensions {
    let data = &properties.world_dimension_data;
    let default_id = "minecraft:normal";
    let key = data.world_preset_key();
    let (preset_id, mut dimensions) = match key.as_ref().and_then(|k| preset_dimensions(context, &k.to_string()).map(|d| (k.to_string(), d))) {
        Some(found) => found,
        None => {
            log::warn!("Failed to parse level-type {}, defaulting to {default_id}", data.level_type);
            (default_id.to_owned(), create_normal_world_dimensions(context))
        }
    };
    if preset_id == "minecraft:flat" {
        let worldgen = Arc::new(context.datapack_world_registries.worldgen_registries());
        let settings_json: Value = serde_json::from_str(&crate::server::players::gson::JsonElement::Object(data.generator_settings.clone()).to_compact_string()).unwrap_or(Value::Null);
        let parsed = BiomeRegistry::load(&worldgen)
            .map_err(|e| e.to_string())
            .and_then(|biomes| mc_worldgen::flat::FlatLevelGeneratorSettings::from_json(&settings_json, &worldgen, &biomes).map_err(|e| e.to_string()));
        match parsed {
            Ok(_) => dimensions.replace_overworld_generator(serde_json::json!({"type": "minecraft:flat", "settings": settings_json})),
            Err(e) => log::error!("{e}"),
        }
    }
    dimensions
}
