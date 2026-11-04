//! `DedicatedServerProperties` and `DedicatedServerSettings`: every `server.properties` key with vanilla's default,
//! parsing, normalization and write-back.

use std::path::{Path, PathBuf};

use mc_core::ident::Identifier;
use mc_core::uuid_util;
use mc_text::Component;
use mc_world::level_storage::{DataPackConfig, Difficulty, GameType, WorldOptions};
use rand::Rng;
use uuid::Uuid;

use super::java_properties::Properties;
use super::settings::{self, MutableValue, Settings, SettingsError};
use crate::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
use crate::server::players::gson::{self, JsonElement, JsonObject};

/// `MinecraftServer.ServerResourcePackInfo`.
#[derive(Clone, Debug)]
pub struct ServerResourcePackInfo {
    pub id: Uuid,
    pub url: String,
    pub hash: String,
    pub is_required: bool,
    pub prompt: Option<Component>,
}

/// `DedicatedServerProperties.WorldDimensionData`: the raw `generator-settings` / `level-type` the world loader
/// turns into dimensions (that needs the registries, so it lives with the world loading code).
#[derive(Clone, Debug)]
pub struct WorldDimensionData {
    pub generator_settings: JsonObject,
    pub level_type: String,
}

impl WorldDimensionData {
    /// The preset key `create` looks up: `Identifier.tryParse(levelType)`, else the legacy names (vanilla only
    /// consults those when the text is not a valid identifier, so in practice never).
    pub fn world_preset_key(&self) -> Option<Identifier> {
        Identifier::try_parse(&self.level_type).or_else(|| match self.level_type.as_str() {
            "default" => Identifier::try_parse("minecraft:normal"),
            "largebiomes" => Identifier::try_parse("minecraft:large_biomes"),
            _ => None,
        })
    }
}

const SECRET_KEY_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// `SecurityConfig.generateSecretKey`.
pub fn generate_secret_key() -> String {
    let mut rng = rand::rng();
    (0..40).map(|_| SECRET_KEY_CHARS[rng.random_range(0..SECRET_KEY_CHARS.len())] as char).collect()
}

/// `Mth.clamp(int, int, int)`.
fn clamp(v: i32, min: i32, max: i32) -> i32 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

/// `Integer.parseInt` escaping as an uncaught `NumberFormatException`.
fn parse_int_or_throw(s: &str) -> Result<i32, SettingsError> {
    s.parse::<i32>().map_err(|_| SettingsError(format!("java.lang.NumberFormatException: For input string: \"{s}\"")))
}

fn game_type_by_name(name: &str) -> Option<GameType> {
    [GameType::Survival, GameType::Creative, GameType::Adventure, GameType::Spectator].into_iter().find(|g| g.get_name() == name)
}

/// `Settings.dispatchNumberOrString`.
fn number_or_string<V>(s: &str, by_id: impl FnOnce(i32) -> V, by_name: impl FnOnce(&str) -> Option<V>) -> Option<V> {
    match s.parse::<i32>() {
        Ok(id) => Some(by_id(id)),
        Err(_) => by_name(s),
    }
}

/// `DedicatedServerProperties.deserializePermission`.
pub fn deserialize_permission(value: &str) -> Option<LevelBasedPermissionSet> {
    value.parse::<i32>().ok().map(|id| LevelBasedPermissionSet::for_level(PermissionLevel::by_id(id)))
}

/// `DedicatedServerProperties.serializePermission`.
pub fn serialize_permission(permission: &LevelBasedPermissionSet) -> String {
    permission.level().id().to_string()
}

/// `Splitter.on(',').trimResults().splitToList` (no empty-string omission: `""` yields `[""]`).
fn comma_split(s: &str) -> Vec<String> {
    s.split(',').map(|p| p.trim().to_owned()).collect()
}

#[derive(Clone, Debug)]
pub struct DedicatedServerProperties {
    properties: Properties,
    pub online_mode: bool,
    pub prevent_proxy_connections: bool,
    pub server_ip: String,
    pub allow_flight: MutableValue<bool>,
    pub motd: MutableValue<String>,
    pub code_of_conduct: bool,
    pub bug_report_link: String,
    pub force_game_mode: MutableValue<bool>,
    pub enforce_whitelist: MutableValue<bool>,
    pub difficulty: MutableValue<Difficulty>,
    pub game_mode: MutableValue<GameType>,
    pub level_name: String,
    pub server_port: i32,
    pub management_server_enabled: bool,
    pub management_server_host: String,
    pub management_server_port: i32,
    pub management_server_secret: String,
    pub management_server_tls_enabled: bool,
    pub management_server_tls_keystore: String,
    pub management_server_tls_keystore_password: String,
    pub management_server_allowed_origins: String,
    pub announce_player_achievements: Option<bool>,
    pub enable_query: bool,
    pub query_port: i32,
    pub enable_rcon: bool,
    pub rcon_port: i32,
    pub rcon_password: String,
    pub hardcore: bool,
    pub use_native_transport: bool,
    pub spawn_protection: MutableValue<i32>,
    pub op_permissions: MutableValue<LevelBasedPermissionSet>,
    pub function_permissions: LevelBasedPermissionSet,
    pub max_tick_time: i64,
    pub max_chained_neighbor_updates: i32,
    pub rate_limit_packets_per_second: i32,
    pub command_spam_threshold_seconds: i32,
    pub chat_spam_threshold_seconds: i32,
    pub view_distance: MutableValue<i32>,
    pub simulation_distance: MutableValue<i32>,
    pub max_players: MutableValue<i32>,
    pub network_compression_threshold: i32,
    pub broadcast_rcon_to_ops: bool,
    pub broadcast_console_to_ops: bool,
    pub max_world_size: i32,
    pub sync_chunk_writes: bool,
    pub region_file_compression: String,
    pub enable_jmx_monitoring: bool,
    pub enable_status: MutableValue<bool>,
    pub hide_online_players: MutableValue<bool>,
    pub entity_broadcast_range_percentage: MutableValue<i32>,
    pub text_filtering_config: String,
    pub text_filtering_version: i32,
    pub server_resource_pack_info: Option<ServerResourcePackInfo>,
    pub initial_data_pack_configuration: DataPackConfig,
    pub player_idle_timeout: MutableValue<i32>,
    pub status_heartbeat_interval: MutableValue<i32>,
    pub white_list: MutableValue<bool>,
    pub enforce_secure_profile: bool,
    pub log_ips: bool,
    pub pause_when_empty_seconds: MutableValue<i32>,
    pub world_dimension_data: WorldDimensionData,
    pub world_options: WorldOptions,
    pub accepts_transfers: MutableValue<bool>,
}

impl DedicatedServerProperties {
    /// `new DedicatedServerProperties(Properties)`. `Err` = an exception vanilla lets propagate (a non-numeric
    /// `entity-broadcast-range-percentage`, malformed `generator-settings` JSON).
    pub fn new(properties: Properties) -> Result<DedicatedServerProperties, SettingsError> {
        let mut s = Settings::new(properties);
        // field initializers run in vanilla's declaration order
        let online_mode = s.get_boolean("online-mode", true);
        let prevent_proxy_connections = s.get_boolean("prevent-proxy-connections", false);
        let server_ip = s.get_string("server-ip", "");
        let allow_flight = s.get_mutable_boolean("allow-flight", false);
        let motd = s.get_mutable_string("motd", "A Minecraft Server");
        let code_of_conduct = s.get_boolean("enable-code-of-conduct", false);
        let bug_report_link = s.get_string("bug-report-link", "");
        let force_game_mode = s.get_mutable_boolean("force-gamemode", false);
        let enforce_whitelist = s.get_mutable_boolean("enforce-whitelist", false);
        let difficulty = s.get_mutable_with(
            "difficulty",
            |v| Ok(number_or_string(v, Difficulty::by_id, Difficulty::by_name)),
            |d| d.get_serialized_name().to_owned(),
            Difficulty::Easy,
        )?;
        let game_mode = s.get_mutable_with(
            "gamemode",
            |v| Ok(number_or_string(v, GameType::by_id, game_type_by_name)),
            |g| g.get_name().to_owned(),
            GameType::Survival,
        )?;
        let level_name = s.get_string("level-name", "world");
        let server_port = s.get_int("server-port", 25565);
        let management_server_enabled = s.get_boolean("management-server-enabled", false);
        let management_server_host = s.get_string("management-server-host", "localhost");
        let management_server_port = s.get_int("management-server-port", 0);
        let management_server_secret = s.get_string("management-server-secret", &generate_secret_key());
        let management_server_tls_enabled = s.get_boolean("management-server-tls-enabled", true);
        let management_server_tls_keystore = s.get_string("management-server-tls-keystore", "");
        let management_server_tls_keystore_password = s.get_string("management-server-tls-keystore-password", "");
        let management_server_allowed_origins = s.get_string("management-server-allowed-origins", "");
        let announce_player_achievements = s.get_legacy_boolean("announce-player-achievements");
        let enable_query = s.get_boolean("enable-query", false);
        let query_port = s.get_int("query.port", 25565);
        let enable_rcon = s.get_boolean("enable-rcon", false);
        let rcon_port = s.get_int("rcon.port", 25575);
        let rcon_password = s.get_string("rcon.password", "");
        let hardcore = s.get_boolean("hardcore", false);
        let use_native_transport = s.get_boolean("use-native-transport", true);
        let spawn_protection = s.get_mutable_int("spawn-protection", 16);
        let op_permissions = s.get_mutable_with(
            "op-permission-level",
            |v| Ok(deserialize_permission(v)),
            serialize_permission,
            LevelBasedPermissionSet::OWNER,
        )?;
        let function_permissions = s.get_with(
            "function-permission-level",
            |v| Ok(deserialize_permission(v)),
            serialize_permission,
            LevelBasedPermissionSet::GAMEMASTER,
        )?;
        let max_tick_time = s.get_long("max-tick-time", 60_000);
        let max_chained_neighbor_updates = s.get_int("max-chained-neighbor-updates", 1_000_000);
        let rate_limit_packets_per_second = s.get_int("rate-limit", 0);
        let command_spam_threshold_seconds = s.get_int("command-spam-threshold-seconds", 10);
        let chat_spam_threshold_seconds = s.get_int("chat-spam-threshold-seconds", 10);
        let view_distance = s.get_mutable_int("view-distance", 10);
        let simulation_distance = s.get_mutable_int("simulation-distance", 10);
        let max_players = s.get_mutable_int("max-players", 20);
        let network_compression_threshold = s.get_int("network-compression-threshold", 256);
        let broadcast_rcon_to_ops = s.get_boolean("broadcast-rcon-to-ops", true);
        let broadcast_console_to_ops = s.get_boolean("broadcast-console-to-ops", true);
        let max_world_size = s.get_int_validated("max-world-size", |v| clamp(v, 1, 29_999_984), 29_999_984);
        let sync_chunk_writes = s.get_boolean("sync-chunk-writes", true);
        let region_file_compression = s.get_string("region-file-compression", "deflate");
        let enable_jmx_monitoring = s.get_boolean("enable-jmx-monitoring", false);
        let enable_status = s.get_mutable_boolean("enable-status", true);
        let hide_online_players = s.get_mutable_boolean("hide-online-players", false);
        let entity_broadcast_range_percentage = s.get_mutable_with(
            "entity-broadcast-range-percentage",
            |v| Ok(Some(clamp(parse_int_or_throw(v)?, 10, 1000))),
            |v| v.to_string(),
            100,
        )?;
        let text_filtering_config = s.get_string("text-filtering-config", "");
        let text_filtering_version = s.get_int("text-filtering-version", 0);
        // the resource pack and pack configuration are read after the world options in the constructor body
        let player_idle_timeout = s.get_mutable_int("player-idle-timeout", 0);
        let status_heartbeat_interval = s.get_mutable_int("status-heartbeat-interval", 0);
        let white_list = s.get_mutable_boolean("white-list", true);
        let enforce_secure_profile = s.get_boolean("enforce-secure-profile", true);
        let log_ips = s.get_boolean("log-ips", true);
        let pause_when_empty_seconds = s.get_mutable_int("pause-when-empty-seconds", 60);
        let accepts_transfers = s.get_mutable_boolean("accepts-transfers", false);

        // constructor body
        let level_seed = s.get_string("level-seed", "");
        let generate_structures = s.get_boolean("generate-structures", true);
        let seed = WorldOptions::parse_seed(&level_seed).unwrap_or_else(rand::random::<i64>);
        let world_options = WorldOptions::new(seed, generate_structures, false);
        let generator_settings = s.get_with(
            "generator-settings",
            |v| {
                gson::parse_object_strict(if v.is_empty() { "{}" } else { v })
                    .map(Some)
                    .map_err(|e| SettingsError(format!("com.google.gson.JsonSyntaxException: {e}")))
            },
            |o| JsonElement::Object(o.clone()).to_compact_string(),
            JsonObject::new(),
        )?;
        let level_type = s.get_with("level-type", |v| Ok(Some(v.to_lowercase())), String::clone, "minecraft:normal".to_owned())?;
        let world_dimension_data = WorldDimensionData { generator_settings, level_type };
        let resource_pack_id = s.get_string("resource-pack-id", "");
        let resource_pack = s.get_string("resource-pack", "");
        let resource_pack_sha1 = s.get_string("resource-pack-sha1", "");
        let resource_pack_hash = s.get_legacy_string("resource-pack-hash");
        let require_resource_pack = s.get_boolean("require-resource-pack", false);
        let resource_pack_prompt = s.get_string("resource-pack-prompt", "");
        let server_resource_pack_info = get_server_pack_info(
            &resource_pack_id,
            &resource_pack,
            &resource_pack_sha1,
            resource_pack_hash.as_deref(),
            require_resource_pack,
            &resource_pack_prompt,
        );
        let default_packs = DataPackConfig::default_config();
        let enabled = s.get_string("initial-enabled-packs", &default_packs.enabled.join(","));
        let disabled = s.get_string("initial-disabled-packs", &default_packs.disabled.join(","));
        let initial_data_pack_configuration = DataPackConfig { enabled: comma_split(&enabled), disabled: comma_split(&disabled) };

        Ok(DedicatedServerProperties {
            properties: s.properties,
            online_mode,
            prevent_proxy_connections,
            server_ip,
            allow_flight,
            motd,
            code_of_conduct,
            bug_report_link,
            force_game_mode,
            enforce_whitelist,
            difficulty,
            game_mode,
            level_name,
            server_port,
            management_server_enabled,
            management_server_host,
            management_server_port,
            management_server_secret,
            management_server_tls_enabled,
            management_server_tls_keystore,
            management_server_tls_keystore_password,
            management_server_allowed_origins,
            announce_player_achievements,
            enable_query,
            query_port,
            enable_rcon,
            rcon_port,
            rcon_password,
            hardcore,
            use_native_transport,
            spawn_protection,
            op_permissions,
            function_permissions,
            max_tick_time,
            max_chained_neighbor_updates,
            rate_limit_packets_per_second,
            command_spam_threshold_seconds,
            chat_spam_threshold_seconds,
            view_distance,
            simulation_distance,
            max_players,
            network_compression_threshold,
            broadcast_rcon_to_ops,
            broadcast_console_to_ops,
            max_world_size,
            sync_chunk_writes,
            region_file_compression,
            enable_jmx_monitoring,
            enable_status,
            hide_online_players,
            entity_broadcast_range_percentage,
            text_filtering_config,
            text_filtering_version,
            server_resource_pack_info,
            initial_data_pack_configuration,
            player_idle_timeout,
            status_heartbeat_interval,
            white_list,
            enforce_secure_profile,
            log_ips,
            pause_when_empty_seconds,
            world_dimension_data,
            world_options,
            accepts_transfers,
        })
    }

    /// `DedicatedServerProperties.fromFile`.
    pub fn from_file(file: &Path) -> Result<DedicatedServerProperties, SettingsError> {
        DedicatedServerProperties::new(settings::load_from_file(file)?)
    }

    /// `MutableValue.update` + `Settings.reload`: these properties with one value replaced.
    pub fn update<V>(&self, field: &MutableValue<V>, value: V) -> DedicatedServerProperties {
        let properties = field.updated_properties(&self.properties, &value);
        // the table was produced by a successful construction and only one numeric/boolean/enum value changed, so
        // re-parsing cannot hit the exceptions `new` reports
        DedicatedServerProperties::new(properties).expect("re-parsing normalized properties")
    }

    /// `Settings.store`.
    pub fn store(&self, output: &Path) {
        settings::store(&self.properties, output)
    }

    /// The normalized property table (what `store` writes).
    pub fn properties(&self) -> &Properties {
        &self.properties
    }
}

/// `DedicatedServerProperties.parseResourcePackPrompt`.
fn parse_resource_pack_prompt(prompt: &str) -> Option<Component> {
    if prompt.is_empty() {
        return None;
    }
    let json: serde_json::Value = match serde_json::from_str(prompt) {
        Ok(j) => j,
        Err(e) => {
            log::warn!("Failed to parse resource pack prompt '{prompt}'\n{e}");
            return None;
        }
    };
    match Component::from_json(&json) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("Failed to parse resource pack prompt '{prompt}': {e}");
            None
        }
    }
}

/// `DedicatedServerProperties.getServerPackInfo`.
fn get_server_pack_info(
    id: &str,
    url: &str,
    sha1: &str,
    legacy_hash: Option<&str>,
    require_resource_pack: bool,
    prompt: &str,
) -> Option<ServerResourcePackInfo> {
    if url.is_empty() {
        return None;
    }
    let legacy_hash = legacy_hash.filter(|h| !h.is_empty());
    let hash = if !sha1.is_empty() {
        if legacy_hash.is_some() {
            log::warn!("resource-pack-hash is deprecated and found along side resource-pack-sha1. resource-pack-hash will be ignored.");
        }
        sha1
    } else if let Some(legacy) = legacy_hash {
        log::warn!("resource-pack-hash is deprecated. Please use resource-pack-sha1 instead.");
        legacy
    } else {
        ""
    };
    if hash.is_empty() {
        log::warn!(
            "You specified a resource pack without providing a sha1 hash. Pack will be updated on the client only if you change the name of the pack."
        );
    } else if !(hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())) {
        log::warn!("Invalid sha1 for resource-pack-sha1");
    }
    let prompt = parse_resource_pack_prompt(prompt);
    let parsed_id = if id.is_empty() {
        let uuid = uuid_util::name_uuid_from_bytes(url.as_bytes());
        log::warn!("resource-pack-id missing, using default of {uuid}");
        uuid
    } else {
        match uuid_util::uuid_from_string(id) {
            Ok(uuid) => uuid,
            Err(_) => {
                log::warn!("Failed to parse '{id}' into UUID");
                return None;
            }
        }
    };
    Some(ServerResourcePackInfo { id: parsed_id, url: url.to_owned(), hash: hash.to_owned(), is_required: require_resource_pack, prompt })
}

/// `DedicatedServerSettings`: the properties plus the file they are persisted to.
pub struct DedicatedServerSettings {
    source: PathBuf,
    properties: DedicatedServerProperties,
}

impl DedicatedServerSettings {
    pub fn new(source: impl Into<PathBuf>) -> Result<DedicatedServerSettings, SettingsError> {
        let source = source.into();
        let properties = DedicatedServerProperties::from_file(&source)?;
        Ok(DedicatedServerSettings { source, properties })
    }

    pub fn get_properties(&self) -> &DedicatedServerProperties {
        &self.properties
    }

    pub fn force_save(&self) {
        self.properties.store(&self.source);
    }

    /// `update`: replace the properties with `mutator(current)` and store them.
    pub fn update(&mut self, mutator: impl FnOnce(&DedicatedServerProperties) -> DedicatedServerProperties) -> &mut Self {
        self.properties = mutator(&self.properties);
        self.properties.store(&self.source);
        self
    }
}
