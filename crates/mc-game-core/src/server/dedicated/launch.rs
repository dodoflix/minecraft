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
use crate::server::MinecraftServer;
use crate::server::level_load_listener::LoggingLevelLoadListener;
use crate::server::minecraft_server::{ReloadableResources, SERVER_THREAD_NAME, ServerInit};
use crate::server::notifications::NotificationManager;
use crate::server::packs::ServerPacksSource;
use crate::server::players::mojang_profile_lookup::MojangProfileLookup;
use crate::server::players::name_resolver::{
    CachedProfileResolver, CachedUserNameToIdResolver, NoProfileLookup, ProfileLookup,
};
use crate::server::shared::ServerShared;
use crate::server::world_dimensions::{DimensionDecoder, WorldDimensions};
use crate::server::world_loader::{
    DataLoadContext, DataLoadOutput, PackConfig, WorldLoadError, WorldStem, load,
};
use mc_nbt::CompoundTag;
use mc_world::level_storage::{
    DifficultySettings, FeatureFlagSet, GameType, LevelResource, LevelSettings, LevelStorageSource,
    PrimaryLevelData, WorldDataConfiguration, WorldGenSettings, WorldOptions, get_data_version,
    requires_file_fixing,
};
use mc_worldgen::flat::FlatLevelGeneratorSettings;
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
    WorldOptions::new(
        i64::from(mc_core::math::java::java_string_hash("North Carolina")),
        true,
        true,
    )
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

/// Everything `start` builds before the server thread is spawned.
struct Prepared {
    shared: Arc<ServerShared>,
    tasks: std::sync::mpsc::Receiver<crate::server::shared::TickTask>,
    init: ServerInit,
    stop_signal: ServerStopSignal,
}

/// Builds the `MinecraftServer` of `options` on the calling thread without running it (tests, tooling): `None` when
/// the launch stops early like `start` does (logged).
pub fn create_server(
    options: ServerOptions,
    settings: DedicatedServerSettings,
) -> Result<Option<MinecraftServer>, String> {
    let Some(Prepared {
        shared,
        tasks,
        init,
        ..
    }) = prepare(&options, settings, Arc::new(NoProfileLookup))?
    else {
        return Ok(None);
    };
    let mut server = MinecraftServer::new(shared, tasks, init)?;
    server.port = options.port;
    server.is_demo = options.demo;
    server.server_id = options.server_id.clone();
    Ok(Some(server))
}

/// `lookup`: the Mojang profile services (`Services.profileResolver()` / `nameToIdCache` repository).
fn prepare(
    options: &ServerOptions,
    settings: DedicatedServerSettings,
    lookup: Arc<dyn ProfileLookup>,
) -> Result<Option<Prepared>, String> {
    let universe = PathBuf::from(&options.universe);
    let name_to_id_cache = Arc::new(CachedUserNameToIdResolver::new(
        lookup.clone(),
        universe.join("usercache.json"),
    ));
    let profile_resolver = Arc::new(CachedProfileResolver::new(lookup, name_to_id_cache.clone()));
    let notification_manager = Arc::new(NotificationManager::new());
    let properties = settings.get_properties().clone();
    let level_name = options
        .world_name
        .clone()
        .unwrap_or_else(|| properties.level_name.clone());
    let level_storage_source =
        LevelStorageSource::create_default(&universe).map_err(|e| e.to_string())?;
    let access = level_storage_source
        .validate_and_create_access(&level_name)
        .map_err(|e| e.to_string())?;

    let level_data_tag: Option<CompoundTag> = if access.has_world_data() {
        let unfixed = match access.get_unfixed_data_tag_with_fallback() {
            Ok(tag) => tag,
            Err(e) => {
                log::error!(
                    "Failed to load world data. World files may be corrupted. Shutting down.\n{e}"
                );
                return Ok(None);
            }
        };
        let summary = match access.fix_and_get_summary_from_tag(&unfixed) {
            Ok(summary) => summary,
            Err(e) => {
                log::error!(
                    "Failed to load world data. World files may be corrupted. Shutting down.\n{e}"
                );
                return Ok(None);
            }
        };
        if summary.requires_manual_conversion {
            log::info!(
                "This world must be opened in an older version (like 1.6.4) to be safely converted"
            );
            return Ok(None);
        }
        if !summary.is_compatible() {
            log::info!("This world was created by an incompatible version.");
            return Ok(None);
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

    let mut pack_repository = ServerPacksSource::create_pack_repository(
        access.get_level_path(LevelResource::DATAPACK_DIR),
        level_storage_source.get_world_dir_validator().clone(),
    );

    let (init_mode, data_config_to_use) = match &level_data_tag {
        Some(tag) => (false, LevelStorageSource::read_data_config(tag)),
        None => (
            true,
            WorldDataConfiguration {
                data_packs: Some(properties.initial_data_pack_configuration.clone()),
                enabled_features: FeatureFlagSet::DEFAULT_FLAGS,
            },
        ),
    };
    let mut pack_config = PackConfig {
        pack_repository: &mut pack_repository,
        initial_data_config: data_config_to_use,
        safe_mode,
        init_mode,
        function_compilation_permissions: properties.function_permissions.into(),
    };
    let loaded = load(
        &mut pack_config,
        |context| {
            world_data_supplier(
                context,
                &access,
                level_data_tag.as_ref(),
                &properties,
                options,
            )
        },
        WorldStem::new,
    );
    let (_selected_config, world_stem) = match loaded {
        Ok(loaded) => loaded,
        Err(e) => {
            log_load_error(&e);
            return Ok(None);
        }
    };

    if options.force_upgrade || options.recreate_region_files {
        log::info!("Forcing world upgrade!");
        log::error!("The world upgrader is not available: continuing without upgrading the world");
    }

    let WorldStem {
        resource_manager,
        data_pack_resources,
        registries,
        world_data,
    } = world_stem;
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
    let dedicated = DedicatedServer::new(settings, console_input, stop_signal.clone())
        .map_err(|e| e.to_string())?;

    let (shared, tasks) = ServerShared::new();
    let init = ServerInit {
        server_directory: PathBuf::new(),
        storage_source: access,
        player_data_storage,
        world_data: data,
        world_gen_settings: gen_settings,
        pack_repository,
        resources: ReloadableResources {
            resource_manager,
            managers: data_pack_resources,
        },
        registries,
        dedicated,
        notification_manager,
        name_to_id_cache,
        profile_resolver,
        level_load_listener: Box::new(LoggingLevelLoadListener::for_dedicated_server()),
    };
    Ok(Some(Prepared {
        shared,
        tasks,
        init,
        stop_signal,
    }))
}

fn start(options: ServerOptions, settings: DedicatedServerSettings) -> Result<i32, String> {
    install_panic_hook();
    let Some(Prepared {
        shared,
        tasks,
        init,
        stop_signal,
    }) = prepare(&options, settings, Arc::new(MojangProfileLookup::new()))?
    else {
        return Ok(0);
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
    let decoder =
        DimensionDecoder::new(&context.datapack_world_registries).unwrap_or_else(|e| panic!("{e}"));
    let datapack_dimensions: Option<&Registry> = context
        .datapack_dimensions
        .registry(keys::LEVEL_STEM)
        .map(|r| r.as_ref());
    let (data, gen_settings, dimensions) = match level_data_tag {
        Some(tag) => {
            let result = LevelStorageSource::get_level_data_and_dimensions(
                access,
                tag,
                context.data_configuration.clone(),
            )
            .unwrap_or_else(|e| panic!("{e}"));
            // `WorldGenSettings.CODEC` decodes the stems too: any failure falls back to the data pack's dimensions
            let stored = result
                .world_gen_settings
                .map_err(|e| e.to_string())
                .and_then(|settings| {
                    let dimensions = WorldDimensions::from_stored(&decoder, &settings.dimensions)?;
                    Ok((
                        WorldGenSettings {
                            options: settings.options,
                            dimensions: dimensions.to_stored(),
                        },
                        dimensions,
                    ))
                });
            let (gen_settings, dimensions) = stored.unwrap_or_else(|e| {
                log::error!("Unable to read or access the world gen settings file! Falling back to the default settings with a random world seed. {e}");
                let dimensions = datapack_dimensions
                    .map(|registry| WorldDimensions::from_registry(&decoder, registry).unwrap_or_else(|e| panic!("{e}")))
                    .expect("the datapack dimensions registry is loaded");
                (WorldGenSettings { options: random_world_options(), dimensions: dimensions.to_stored() }, dimensions)
            });
            (result.world_data, gen_settings, dimensions)
        }
        None => {
            log::info!("No existing world data, creating new world");
            create_new_world_data(
                &decoder,
                context,
                properties,
                options.demo,
                options.bonus_chest,
            )
        }
    };
    let complete = dimensions
        .bake(&decoder, datapack_dimensions)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut data = data;
    data.special_world_property = complete.special_world_property;
    DataLoadOutput {
        cookie: WorldDataAndGenSettings { data, gen_settings },
        final_dimensions: complete.dimensions,
    }
}

fn random_world_options() -> WorldOptions {
    use rand::Rng;
    WorldOptions::new(rand::rng().random::<i64>(), true, false)
}

/// `WorldPresets.createNormalWorldDimensions`.
fn create_normal_world_dimensions(
    decoder: &DimensionDecoder,
    context: &DataLoadContext,
) -> WorldDimensions {
    preset_dimensions(decoder, context, "minecraft:normal")
        .unwrap_or_else(|| panic!("Invalid datapack contents: can't find default preset"))
}

fn preset_dimensions(
    decoder: &DimensionDecoder,
    context: &DataLoadContext,
    id: &str,
) -> Option<WorldDimensions> {
    let id = mc_core::ident::Identifier::try_parse(id)?;
    let registry = context
        .datapack_world_registries
        .registry(keys::WORLD_PRESET)?;
    let entry = registry.entry_by_key(&id)?;
    match WorldDimensions::from_preset_json(decoder, &entry.json) {
        Ok(dimensions) => Some(dimensions),
        Err(e) => {
            log::error!("Failed to decode the world preset {id}: {e}");
            None
        }
    }
}

/// `Main.createNewWorldData`.
fn create_new_world_data(
    decoder: &DimensionDecoder,
    context: &DataLoadContext,
    properties: &DedicatedServerProperties,
    demo_mode: bool,
    bonus_chest: bool,
) -> (PrimaryLevelData, WorldGenSettings, WorldDimensions) {
    let (level_settings, world_options, dimensions) = if demo_mode {
        (
            demo_settings(context.data_configuration.clone()),
            demo_world_options(),
            create_normal_world_dimensions(decoder, context),
        )
    } else {
        let level_settings = LevelSettings {
            level_name: properties.level_name.clone(),
            game_type: *properties.game_mode.get(),
            difficulty_settings: DifficultySettings {
                difficulty: *properties.difficulty.get(),
                hardcore: properties.hardcore,
                locked: false,
            },
            allow_commands: false,
            data_configuration: context.data_configuration.clone(),
        };
        let mut world_options = properties.world_options.clone();
        if bonus_chest {
            world_options.generate_bonus_chest = true;
        }
        (
            level_settings,
            world_options,
            create_dimensions(decoder, context, properties),
        )
    };
    let special = dimensions.special_world_property();
    let data = PrimaryLevelData::new(level_settings, special);
    let gen_settings = WorldGenSettings {
        options: world_options,
        dimensions: dimensions.to_stored(),
    };
    (data, gen_settings, dimensions)
}

/// `DedicatedServerProperties.WorldDimensionData.create`: the `level-type` preset; for the flat preset the overworld
/// generator is replaced by one built from `generator-settings`.
fn create_dimensions(
    decoder: &DimensionDecoder,
    context: &DataLoadContext,
    properties: &DedicatedServerProperties,
) -> WorldDimensions {
    let data = &properties.world_dimension_data;
    let default_id = "minecraft:normal";
    let key = data.world_preset_key();
    let (preset_id, mut dimensions) = match key.as_ref().and_then(|k| {
        preset_dimensions(decoder, context, &k.to_string()).map(|d| (k.to_string(), d))
    }) {
        Some(found) => found,
        None => {
            log::warn!(
                "Failed to parse level-type {}, defaulting to {default_id}",
                data.level_type
            );
            (
                default_id.to_owned(),
                create_normal_world_dimensions(decoder, context),
            )
        }
    };
    if preset_id == "minecraft:flat" {
        let settings_json: Value = serde_json::from_str(
            &crate::server::players::gson::JsonElement::Object(data.generator_settings.clone())
                .to_compact_string(),
        )
        .unwrap_or(Value::Null);
        // `resultOrPartial(LOGGER::error)`: a decoding error is logged, and settings that only failed `validateHeight` are still used
        match FlatLevelGeneratorSettings::from_json_partial(
            &settings_json,
            &decoder.worldgen,
            &decoder.biomes,
        ) {
            Ok((settings, height_error)) => {
                if let Some(e) = height_error {
                    log::error!("{e}");
                }
                match decoder.flat_generator(settings, &settings_json).and_then(
                    |(generator, tag)| {
                        dimensions.replace_overworld_generator(decoder, generator, tag)
                    },
                ) {
                    Ok(()) => {}
                    Err(e) => log::error!("{e}"),
                }
            }
            Err(e) => log::error!("{e}"),
        }
    }
    dimensions
}
