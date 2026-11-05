//! `net.minecraft.server.dedicated.DedicatedServer`: the overrides of `MinecraftServer` for the dedicated server, as
//! inherent methods of [`MinecraftServer`] (there is no integrated server), plus the state they need
//! ([`DedicatedServer`], held in `MinecraftServer::dedicated`).

use super::console::{ConsoleInputQueue, spawn_console_thread};
use super::properties::{DedicatedServerProperties, DedicatedServerSettings};
use super::stop::ServerStopSignal;
use crate::crash_report::SystemReport;
use crate::server::MinecraftServer;
use crate::server::notifications::NotificationService;
use crate::server::minecraft_server::VANILLA_BRAND;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::server::players::UserNameToIdResolver;
use mc_world::level_storage::{Difficulty, GameType};
use std::collections::HashMap;
use crate::server::rcon::{QueryServerInfo, QueryThreadGs4, RconThread};
use std::io;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `DedicatedServer`'s own fields.
pub struct DedicatedServer {
    pub settings: DedicatedServerSettings,
    pub console_input: ConsoleInputQueue,
    pub stop_signal: ServerStopSignal,
    /// `codeOfConductTexts` (language -> text), empty unless `enable-code-of-conduct`.
    pub code_of_conduct_texts: HashMap<String, String>,
    pub query_thread: Option<QueryThreadGs4>,
    pub rcon_thread: Option<RconThread>,
    pub management_server: Option<crate::server::jsonrpc::ManagementServer>,
}

impl DedicatedServer {
    pub fn new(settings: DedicatedServerSettings, console_input: ConsoleInputQueue, stop_signal: ServerStopSignal) -> io::Result<Self> {
        let code_of_conduct_texts = if settings.get_properties().code_of_conduct { read_code_of_conducts()? } else { HashMap::new() };
        Ok(Self { settings, console_input, stop_signal, code_of_conduct_texts, query_thread: None, rcon_thread: None, management_server: None })
    }
}

/// `DedicatedServer.readCodeOfConducts`: `codeofconduct/<language>.txt` files (no symlinks out of the folder).
fn read_code_of_conducts() -> io::Result<HashMap<String, String>> {
    let path = Path::new("codeofconduct");
    let is_dir = std::fs::symlink_metadata(path).map(|m| m.is_dir()).unwrap_or(false);
    if !is_dir {
        return Err(io::Error::other(format!("Code of Conduct folder does not exist: {}", path.display())));
    }
    let canonical_root = std::path::absolute(path)?;
    let mut texts = HashMap::new();
    for entry in std::fs::read_dir(path).map_err(|e| io::Error::other(format!("Failed to read Code of Conduct folder: {e}")))? {
        let file = entry?.path();
        let filename = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(stem) = filename.strip_suffix(".txt") else { continue };
        let real_parent = std::fs::canonicalize(&file)?.parent().map(Path::to_path_buf);
        if real_parent.as_deref() != Some(canonical_root.as_path()) && real_parent != std::fs::canonicalize(&canonical_root).ok() {
            return Err(io::Error::other(format!(
                "Failed to read Code of Conduct file \"{filename}\" because it links to a file outside the allowed directory"
            )));
        }
        let text = std::fs::read_to_string(&file)
            .map_err(|e| io::Error::other(format!("Failed to read Code of Conduct file {filename}: {e}")))?
            .lines()
            .collect::<Vec<_>>()
            .join("\n");
        texts.insert(stem.to_lowercase(), strip_color(&text));
    }
    Ok(texts)
}

/// `StringUtil.stripColor`: removes `§x` formatting codes.
fn strip_color(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

impl MinecraftServer {
    pub fn properties(&self) -> &DedicatedServerProperties {
        self.dedicated.settings.get_properties()
    }

    // ------------------------------------------------------------------ initServer

    /// `DedicatedServer.initServer`: everything between the thread start and the first tick. `Ok(false)` = failed to
    /// initialize (vanilla: `IllegalStateException("Failed to initialize server")`).
    pub(in crate::server) fn init_server(&mut self) -> io::Result<bool> {
        crate::commands::BrigadierExceptions::install();
        crate::commands::install_text_validators();
        mc_protocol::components::install_text_hooks();
        if properties_management_enabled(self.properties()) {
            let api = Arc::new(crate::server::management_api::ServerManagementApi::new(self.shared.clone()));
            match crate::server::jsonrpc::create(self.properties(), &self.notification_manager, api) {
                Ok(server) => self.dedicated.management_server = server,
                Err(e) => return Err(io::Error::other(e)),
            }
        }
        spawn_console_thread(self.dedicated.console_input.clone(), self.dedicated.stop_signal.clone())?;
        log::info!("Starting minecraft server version {}", mc_world::level_storage::WORLD_VERSION_NAME);
        log::info!("Loading properties");
        let properties = self.properties().clone();
        // not singleplayer
        self.online_mode = properties.online_mode;
        self.prevent_proxy_connections = properties.prevent_proxy_connections;
        self.local_ip = properties.server_ip.clone();
        self.player_list.settings = super::super::minecraft_server::player_list_settings(&properties);

        let game_mode = *properties.game_mode.get();
        self.world_data.lock().settings.game_type = game_mode;
        log::info!("Default game type: {}", game_type_name(game_mode));
        let _local_address: Option<IpAddr> = if self.local_ip.is_empty() {
            None
        } else {
            Some(
                (self.local_ip.as_str(), 0)
                    .to_socket_addrs()
                    .map_err(|e| io::Error::other(format!("{}: {e}", self.local_ip)))?
                    .next()
                    .map(|a| a.ip())
                    .ok_or_else(|| io::Error::other(format!("{}: Name or service not known", self.local_ip)))?,
            )
        };
        if self.port < 0 {
            self.port = properties.server_port;
        }

        self.initialize_key_pair()?;
        log::info!("Starting Minecraft server on {}:{}", if self.local_ip.is_empty() { "*" } else { &self.local_ip }, self.port);
        self.start_connection_listener(_local_address)?;

        if !self.uses_authentication() {
            log::warn!("**** SERVER IS RUNNING IN OFFLINE/INSECURE MODE!");
            log::warn!("The server will make no attempt to authenticate usernames. Beware.");
            log::warn!(
                "While this makes the game possible to play without internet access, it also opens up the ability for hackers to connect with any username they choose."
            );
            log::warn!("To change this, set \"online-mode\" to \"true\" in the server.properties file.");
        }

        if self.convert_old_users() {
            self.name_to_id_cache.save();
        }
        if !crate::server::players::old_users_converter::are_old_userlists_removed(&self.server_directory_or_cwd()) {
            return Ok(false);
        }

        let level_nano_time = crate::server::shared::get_nanos();
        self.name_to_id_cache.resolve_offline_users(!self.uses_authentication());
        log::info!("Preparing level \"{}\"", self.get_level_id_name());
        self.load_level();
        let elapsed = crate::server::shared::get_nanos() - level_nano_time;
        log::info!("Done ({:.3}s)! For help, type \"help\"", elapsed as f64 / 1.0e9);
        if let Some(announce) = properties.announce_player_achievements {
            self.set_game_rule(crate::world::level::gamerules::GameRules::SHOW_ADVANCEMENT_MESSAGES, announce);
        }

        if properties.enable_query {
            log::info!("Starting GS4 status listener");
            let info: Arc<dyn QueryServerInfo> = Arc::new(ServerHandle(self.shared.clone()));
            self.dedicated.query_thread = QueryThreadGs4::create(info, &self.local_ip, properties.query_port);
        }
        if properties.enable_rcon {
            log::info!("Starting remote control listener");
            let runner = Arc::new(crate::server::management_api::ServerRconRunner::new(self.shared.clone(), properties.broadcast_rcon_to_ops));
            self.dedicated.rcon_thread = RconThread::create(runner, &self.local_ip, properties.rcon_port, &properties.rcon_password);
        }
        if self.get_max_tick_length() > 0 {
            crate::server::watchdog::spawn(self.shared.clone(), self.watchdog_info.clone(), self.get_max_tick_length(), self.server_directory.clone())?;
        }
        // JMX monitoring (`enable-jmx-monitoring`) is a JVM facility: not applicable.

        self.save_everything(false, true, true);
        self.notification_manager.server_started();
        Ok(true)
    }

    fn server_directory_or_cwd(&self) -> PathBuf {
        if self.server_directory.as_os_str().is_empty() { PathBuf::from(".") } else { self.server_directory.clone() }
    }

    /// `MinecraftServer.initializeKeyPair`.
    fn initialize_key_pair(&mut self) -> io::Result<()> {
        log::info!("Generating keypair");
        let pair = mc_protocol::auth::ServerKeyPair::generate().map_err(|e| io::Error::other(format!("Failed to generate key pair: {e}")))?;
        self.key_pair = Some(Arc::new(pair));
        Ok(())
    }

    /// `DedicatedServer.convertOldUsers`: the five `OldUsersConverter` conversions, each retried twice with a 5 s
    /// pause.
    fn convert_old_users(&mut self) -> bool {
        use crate::server::players::old_users_converter as converter;
        let ctx = converter::ConversionContext {
            dir: self.server_directory_or_cwd(),
            uses_authentication: self.uses_authentication(),
            is_singleplayer: false,
            profile_lookup: Arc::new(crate::server::players::name_resolver::NoProfileLookup),
            name_to_id_cache: self.name_to_id_cache.clone(),
            operator_permissions: self.operator_user_permissions(),
        };
        let world_dir = self.storage_source.get_level_path(mc_world::level_storage::LevelResource::ROOT);
        let retry = |what: &str, convert: &dyn Fn() -> bool| {
            let mut done = false;
            for retries in 0..=2 {
                if done {
                    break;
                }
                if retries > 0 {
                    log::warn!("Encountered a problem while converting the {what}, retrying in a few seconds");
                    std::thread::sleep(std::time::Duration::from_millis(5000));
                }
                done = convert();
            }
            done
        };
        let user_banlist = retry("user banlist", &|| converter::convert_user_banlist(&ctx));
        let ip_banlist = retry("ip banlist", &|| converter::convert_ip_banlist(&ctx));
        let ops_list = retry("op list", &|| converter::convert_ops_list(&ctx));
        let whitelist = retry("whitelist", &|| converter::convert_white_list(&ctx));
        let players = retry("player save files", &|| converter::convert_players(&ctx, &world_dir));
        user_banlist || ip_banlist || ops_list || whitelist || players
    }


    // ------------------------------------------------------------------ DedicatedServer overrides

    pub fn is_dedicated_server(&self) -> bool {
        true
    }

    /// `getMotd`.
    pub fn get_motd(&self) -> String {
        self.properties().motd.get().clone()
    }

    pub fn set_motd(&mut self, motd: &str) {
        self.dedicated.settings.update(|p| p.update(&p.motd, motd.to_owned()));
    }

    pub fn get_max_players(&self) -> i32 {
        *self.properties().max_players.get()
    }

    pub fn set_max_players(&mut self, max_players: i32) {
        self.dedicated.settings.update(|p| p.update(&p.max_players, max_players));
    }

    pub fn is_enforce_whitelist(&self) -> bool {
        *self.properties().enforce_whitelist.get()
    }

    pub fn set_enforce_whitelist(&mut self, enforce_whitelist: bool) {
        self.dedicated.settings.update(|p| p.update(&p.enforce_whitelist, enforce_whitelist));
    }

    pub fn is_using_whitelist(&self) -> bool {
        *self.properties().white_list.get()
    }

    pub fn set_using_whitelist(&mut self, using_whitelist: bool) {
        self.dedicated.settings.update(|p| p.update(&p.white_list, using_whitelist));
    }

    pub fn allow_flight(&self) -> bool {
        *self.properties().allow_flight.get()
    }

    pub fn set_allow_flight(&mut self, allowed: bool) {
        self.dedicated.settings.update(|p| p.update(&p.allow_flight, allowed));
    }

    /// `DedicatedServer.setDifficulty(Difficulty)`: persists, then `forceDifficulty`.
    pub fn set_difficulty_property(&mut self, difficulty: Difficulty) {
        self.dedicated.settings.update(|p| p.update(&p.difficulty, difficulty));
        self.force_difficulty();
    }

    /// `forceDifficulty` (dedicated override).
    pub(in crate::server) fn force_difficulty(&mut self) {
        let difficulty = *self.properties().difficulty.get();
        self.set_difficulty(difficulty, true);
    }

    pub fn view_distance(&self) -> i32 {
        *self.properties().view_distance.get()
    }

    pub fn set_view_distance(&mut self, view_distance: i32) {
        self.dedicated.settings.update(|p| p.update(&p.view_distance, view_distance));
        crate::server::players::player_ops::set_view_distance(self, view_distance);
    }

    pub fn set_simulation_distance(&mut self, simulation_distance: i32) {
        self.dedicated.settings.update(|p| p.update(&p.simulation_distance, simulation_distance));
        crate::server::players::player_ops::set_simulation_distance(self, simulation_distance);
    }

    pub fn simulation_distance(&self) -> i32 {
        *self.properties().simulation_distance.get()
    }

    pub fn spawn_protection_radius(&self) -> i32 {
        *self.properties().spawn_protection.get()
    }

    pub fn set_spawn_protection_radius(&mut self, radius: i32) {
        self.dedicated.settings.update(|p| p.update(&p.spawn_protection, radius));
    }

    pub fn replies_to_status(&self) -> bool {
        *self.properties().enable_status.get()
    }

    pub fn set_replies_to_status(&mut self, enable: bool) {
        self.dedicated.settings.update(|p| p.update(&p.enable_status, enable));
    }

    pub fn hides_online_players(&self) -> bool {
        *self.properties().hide_online_players.get()
    }

    pub fn set_hides_online_players(&mut self, hide: bool) {
        self.dedicated.settings.update(|p| p.update(&p.hide_online_players, hide));
    }

    pub fn operator_user_permissions(&self) -> LevelBasedPermissionSet {
        self.properties().op_permissions.get().clone()
    }

    pub fn set_operator_user_permissions(&mut self, permissions: LevelBasedPermissionSet) {
        self.dedicated.settings.update(|p| p.update(&p.op_permissions, permissions));
    }

    pub fn get_function_compilation_permissions(&self) -> LevelBasedPermissionSet {
        self.properties().function_permissions.clone()
    }

    /// `playerIdleTimeout`.
    pub fn player_idle_timeout(&self) -> i32 {
        *self.properties().player_idle_timeout.get()
    }

    pub fn set_player_idle_timeout(&mut self, timeout: i32) {
        self.dedicated.settings.update(|p| p.update(&p.player_idle_timeout, timeout));
    }

    pub fn entity_broadcast_range_percentage(&self) -> i32 {
        *self.properties().entity_broadcast_range_percentage.get()
    }

    pub fn set_entity_broadcast_range_percentage(&mut self, range: i32) {
        self.dedicated.settings.update(|p| p.update(&p.entity_broadcast_range_percentage, range));
    }

    /// `getScaledTrackingDistance`.
    pub fn get_scaled_tracking_distance(&self, range: i32) -> i32 {
        self.entity_broadcast_range_percentage().wrapping_mul(range) / 100
    }

    pub fn accepts_transfers(&self) -> bool {
        *self.properties().accepts_transfers.get()
    }

    pub fn set_accepts_transfers(&mut self, accept: bool) {
        self.dedicated.settings.update(|p| p.update(&p.accepts_transfers, accept));
        if let Some(hooks) = &self.network_hooks {
            hooks.update_settings(|s| s.accepts_transfers = accept);
        }
    }

    /// `pauseWhenEmptySeconds`.
    pub fn pause_when_empty_seconds(&self) -> i32 {
        *self.properties().pause_when_empty_seconds.get()
    }

    pub fn set_pause_when_empty_seconds(&mut self, seconds: i32) {
        self.dedicated.settings.update(|p| p.update(&p.pause_when_empty_seconds, seconds));
    }

    pub fn force_game_mode(&self) -> bool {
        *self.properties().force_game_mode.get()
    }

    /// `getForcedGameType`.
    pub fn get_forced_game_type(&self) -> Option<GameType> {
        self.force_game_mode().then_some(self.world_data.lock().settings.game_type)
    }

    pub fn game_mode(&self) -> GameType {
        *self.properties().game_mode.get()
    }

    /// `getMaxTickLength` (`max-tick-time`, ms).
    pub fn get_max_tick_length(&self) -> i64 {
        self.properties().max_tick_time
    }

    pub fn get_max_chained_neighbor_updates(&self) -> i32 {
        self.properties().max_chained_neighbor_updates
    }

    pub fn get_rate_limit_packets_per_second(&self) -> i32 {
        self.properties().rate_limit_packets_per_second
    }

    pub fn get_command_spam_threshold_seconds(&self) -> i32 {
        self.properties().command_spam_threshold_seconds
    }

    pub fn get_chat_spam_threshold_seconds(&self) -> i32 {
        self.properties().chat_spam_threshold_seconds
    }

    pub fn use_native_transport(&self) -> bool {
        self.properties().use_native_transport
    }

    pub fn should_rcon_broadcast(&self) -> bool {
        self.properties().broadcast_rcon_to_ops
    }

    pub fn should_inform_admins(&self) -> bool {
        self.properties().broadcast_console_to_ops
    }

    pub fn get_absolute_max_world_size(&self) -> i32 {
        self.properties().max_world_size
    }

    pub fn get_compression_threshold(&self) -> i32 {
        self.properties().network_compression_threshold
    }

    /// `enforceSecureProfile`: needs online mode and profile-key validation.
    pub fn enforce_secure_profile(&self) -> bool {
        let properties = self.properties();
        properties.enforce_secure_profile && properties.online_mode
    }

    pub fn log_ips(&self) -> bool {
        self.properties().log_ips
    }

    pub fn force_synchronous_writes(&self) -> bool {
        self.properties().sync_chunk_writes
    }

    pub fn uses_authentication(&self) -> bool {
        self.online_mode
    }

    /// `getLevelIdName`.
    pub fn get_level_id_name(&self) -> String {
        self.storage_source.get_level_id().to_owned()
    }

    pub fn get_server_ip(&self) -> &str {
        &self.local_ip
    }

    pub fn get_server_port(&self) -> i32 {
        self.port
    }

    pub fn get_plugin_names(&self) -> &'static str {
        ""
    }

    pub fn is_single_player_owner(&self, _name_and_id: &crate::server::players::NameAndId) -> bool {
        false
    }

    /// `fillServerSystemReport`.
    pub(in crate::server) fn fill_system_report_details(&self, report: &mut SystemReport) {
        report.set_detail("Is Modded", if self.world_data.lock().was_modded { "Definitely; Server brand changed to '" } else { "Probably not. Server jar signature and brand is untouched" });
        report.set_detail("Type", "Dedicated Server");
    }

    /// `getServerModName` / the brand reported to clients.
    pub fn get_server_mod_name(&self) -> &'static str {
        VANILLA_BRAND
    }

    /// `dumpServerProperties` (written by `/perf` reports).
    pub fn dump_server_properties(&self, path: &Path) -> io::Result<()> {
        use std::io::Write;
        let p = self.properties();
        let mut out = io::BufWriter::new(std::fs::File::create(path)?);
        writeln!(out, "sync-chunk-writes={}", p.sync_chunk_writes)?;
        writeln!(out, "gamemode={}", game_type_name(*p.game_mode.get()))?;
        writeln!(out, "entity-broadcast-range-percentage={}", p.entity_broadcast_range_percentage.get())?;
        writeln!(out, "max-world-size={}", p.max_world_size)?;
        writeln!(out, "view-distance={}", p.view_distance.get())?;
        writeln!(out, "simulation-distance={}", p.simulation_distance.get())?;
        writeln!(out, "generate-structures={}", p.world_options.generate_structures)?;
        writeln!(out, "use-native={}", p.use_native_transport)?;
        writeln!(out, "rate-limit={}", p.rate_limit_packets_per_second)?;
        writeln!(out, "command-spam-threshold-seconds={}", p.command_spam_threshold_seconds)?;
        writeln!(out, "chat-spam-threshold-seconds={}", p.chat_spam_threshold_seconds)?;
        out.flush()
    }

    /// `tickConnection` (dedicated override): connection tick, then the console input.
    pub(in crate::server) fn tick_connection(&mut self) {
        // The listener ticks the play connections with `&mut MinecraftServer`: take it out for the call.
        if let Some(mut connection) = self.connection.take() {
            connection.tick(self);
            self.connection = Some(connection);
        }
        self.handle_console_inputs();
    }

    /// The network layer's view of the server settings (`server.properties` + runtime changes).
    fn network_settings(&self) -> crate::server::network::NetworkSettings {
        let p = self.properties();
        crate::server::network::NetworkSettings {
            uses_authentication: self.uses_authentication(),
            compression_threshold: p.network_compression_threshold,
            rate_limit_packets_per_second: p.rate_limit_packets_per_second,
            prevent_proxy_connections: p.prevent_proxy_connections,
            accepts_transfers: *p.accepts_transfers.get(),
            log_ips: p.log_ips,
            replies_to_status: *p.enable_status.get(),
            singleplayer_profile: None,
            server_mod_name: VANILLA_BRAND.to_owned(),
        }
    }

    /// `getConnection().startTcpServerListener(localAddress, port)`.
    fn start_connection_listener(&mut self, local_address: Option<IpAddr>) -> io::Result<()> {
        use crate::server::network::server_configuration_data::configuration_data;
        use crate::server::network::server_network_hooks::ServerNetworkHooks;
        let key_pair = self.key_pair.clone().ok_or_else(|| io::Error::other("the key pair is generated before the listener starts"))?;
        let hooks = Arc::new(ServerNetworkHooks::new(
            self.shared.clone(),
            self.network_settings(),
            key_pair,
            mc_protocol::auth::SessionClient::default(),
            Arc::new(configuration_data(self)),
        ));
        let mut connection = crate::server::network::ServerConnectionListener::new(hooks.clone())?;
        connection.set_play_connection_ticker(crate::server::network::game_listener::tick_connection);
        connection.start_tcp_server_listener(local_address, self.port as u16)?;
        // the world side of the hooks: the player list's login checks come with `PlayerListHooks`, the spawn task and
        // the activity monitor (`new ServerActivityMonitor(notificationManager, 30)`) are installed here
        hooks.player_list_hooks().set_prepare_spawn(crate::server::players::prepare_spawn::factory());
        hooks.player_list_hooks().set_activity_monitor(Arc::new(crate::server::notifications::ServerActivityMonitor::new(self.notification_manager.clone(), 30)));
        self.network_hooks = Some(hooks);
        self.connection = Some(connection);
        Ok(())
    }

    /// `handleConsoleInputs`: every queued console line runs as a command of the server console source.
    pub fn handle_console_inputs(&mut self) {
        for line in self.dedicated.console_input.drain() {
            let source = self.create_command_source_stack();
            let commands = self.commands.clone();
            commands.perform_prefixed_command(self, source, &line);
        }
    }

    /// `stopServer`'s `getConnection().stop()`.
    pub(in crate::server) fn stop_connection(&mut self) {
        if let Some(mut connection) = self.connection.take() {
            connection.stop();
        }
    }
    /// `createCommandSourceStack`: the console (`Server`, owner permissions, at the respawn position of the respawn
    /// dimension).
    pub fn create_command_source_stack(&self) -> crate::commands::CommandSourceStack {
        let respawn = self.get_respawn_data();
        let level = self.find_respawn_dimension();
        crate::commands::CommandSourceStack::named(
            Arc::new(crate::commands::console_command_source::ConsoleCommandSource),
            mc_core::phys::Vec3::new(f64::from(respawn.pos.x()), f64::from(respawn.pos.y()), f64::from(respawn.pos.z())),
            mc_core::phys::Vec2::ZERO,
            level,
            crate::server::permissions::LevelBasedPermissionSet::OWNER.into(),
            mc_text::Component::literal("Server"),
        )
    }


    /// `onServerExit`.
    pub(in crate::server) fn on_server_exit(&mut self) {
        // Text filter, GUI, rcon/query threads and the management server stop here.
        if let Some(mut rcon) = self.dedicated.rcon_thread.take() {
            rcon.stop();
        }
        if let Some(mut query) = self.dedicated.query_thread.take() {
            query.stop();
        }
        if let Some(mut management) = self.dedicated.management_server.take() {
            management.stop();
        }
    }

    /// `onServerCrash` (nothing for the dedicated server).
    pub(in crate::server) fn on_server_crash(&mut self, _report: &crate::crash_report::CrashReport) {}

    /// `MinecraftServer.buildServerStatus`.
    pub(in crate::server) fn build_server_status(&mut self) -> mc_protocol::packets::status::ServerStatus {
        let handles = self.player_list.get_players();
        let players: Vec<crate::server::status::StatusPlayer> = handles
            .iter()
            .map(|p| crate::server::status::StatusPlayer { name_and_id: (p.uuid, p.name().to_owned()), allows_listing: p.allows_listing() })
            .collect();
        let max_players = self.get_max_players();
        let hides = self.hides_online_players();
        let sample = crate::server::status::build_player_status(&players, max_players, hides, self.status_random());
        let motd = self.get_motd();
        let enforce = self.enforce_secure_profile();
        crate::server::status::build_server_status(&motd, sample, self.status_icon(), enforce)
    }

    /// Publish the values off-thread readers (query, rcon, management API) see.
    pub(in crate::server) fn refresh_shared_info(&mut self) {
        let names = self.player_list.get_player_names_array();
        let snapshot = crate::server::shared::ServerInfoSnapshot {
            motd: self.get_motd(),
            server_version: mc_world::level_storage::WORLD_VERSION_NAME.to_owned(),
            player_count: self.player_list.get_player_count(),
            max_players: self.get_max_players(),
            player_names: names,
            server_ip: self.local_ip.clone(),
            server_port: self.port,
            level_id_name: self.get_level_id_name(),
        };
        self.shared.set_info(snapshot);
        let mut info = self.watchdog_info.lock();
        info.milliseconds_per_tick = self.tick_rate_manager.milliseconds_per_tick();
    }
}

/// What the query listener reads: the values `refresh_shared_info` publishes.
struct ServerHandle(Arc<crate::server::shared::ServerShared>);

impl QueryServerInfo for ServerHandle {
    fn server_port(&self) -> i32 {
        self.0.info().server_port
    }
    fn server_name(&self) -> String {
        self.0.info().motd
    }
    fn max_players(&self) -> i32 {
        self.0.info().max_players
    }
    fn level_id_name(&self) -> String {
        self.0.info().level_id_name
    }
    fn player_count(&self) -> i32 {
        self.0.info().player_count
    }
    fn player_names(&self) -> Vec<String> {
        self.0.info().player_names
    }
    fn server_version(&self) -> String {
        self.0.info().server_version
    }
}

/// `GameType.toString()` (the Java enum constant name).
fn game_type_name(game_type: GameType) -> String {
    game_type.get_name().to_uppercase()
}

fn properties_management_enabled(properties: &DedicatedServerProperties) -> bool {
    properties.management_server_enabled
}
