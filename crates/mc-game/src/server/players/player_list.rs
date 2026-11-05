//! `net.minecraft.server.players.PlayerList` (+ `DedicatedPlayerList`'s overrides, the dedicated server being the only
//! one). The list holds [`PlayerHandle`]s, not entities: a player is taken out of its level while its packet or tick is
//! handled, but a broadcast must still reach it (see `player_handle`). Operations that need the server (`broadcast*`,
//! `op`, `setViewDistance`, ...) are free functions over `&mut MinecraftServer`, because a method cannot borrow
//! `server.player_list` and `server` at once (ARCHITECTURE: tier 2).

use super::name_and_id::NameAndId;
use super::player_files::{PlayerAdvancementsFile, locate_stats_file};
use crate::stats::{ServerStatsCounter, SharedStats};
use super::player_handle::PlayerHandle;
use super::user_lists::{IpBanList, ServerOpList, ServerOpListEntry, UserBanList, UserWhiteList};
use crate::server::chat::chat_type::Bound;
use crate::server::chat::{OutgoingChatMessage, PlayerChatMessage, instant_now};
use crate::server::notifications::NotificationService;
use crate::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
use crate::world::level::DimensionKey;
use mc_core::phys::Vec3;
use mc_nbt::CompoundTag;
use mc_protocol::codec::EnumSet;
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{
    ClientboundPlayerInfoRemovePacket, ClientboundPlayerInfoUpdateAction,
};
use mc_text::{ChatFormatting, Component, TranslatableArg};
use mc_world::level_storage::PlayerDataStorage;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// `PlayerList.USERBANLIST_FILE` and friends (relative to the working directory, like vanilla's `new File(..)`).
pub const USERBANLIST_FILE: &str = "banned-players.json";
pub const IPBANLIST_FILE: &str = "banned-ips.json";
pub const OPLIST_FILE: &str = "ops.json";
pub const WHITELIST_FILE: &str = "whitelist.json";

/// `SEND_PLAYER_INFO_INTERVAL`.
const SEND_PLAYER_INFO_INTERVAL: i32 = 600;

/// `PlayerList.CHAT_FILTERED_FULL`.
pub fn chat_filtered_full() -> Component {
    Component::translatable("chat.filtered_full", vec![])
}

/// `PlayerList.DUPLICATE_LOGIN_DISCONNECT_MESSAGE`.
pub fn duplicate_login_disconnect_message() -> Component {
    Component::translatable("multiplayer.disconnect.duplicate_login", vec![])
}

/// The `MinecraftServer`/`DedicatedServer` values the player list reads (`getMaxPlayers`, `isUsingWhitelist`,
/// `enforceSecureProfile`, `usesAuthentication`, `logIPs`, `operatorUserPermissions`). The dedicated settings own the
/// authoritative values; whoever changes one (properties update, `/whitelist on`) writes it here too.
#[derive(Clone, Debug)]
pub struct PlayerListSettings {
    pub max_players: i32,
    pub using_whitelist: bool,
    pub enforce_secure_profile: bool,
    pub uses_authentication: bool,
    pub log_ips: bool,
    pub operator_user_permissions: LevelBasedPermissionSet,
}

impl Default for PlayerListSettings {
    /// `server.properties` defaults.
    fn default() -> Self {
        Self {
            max_players: 20,
            using_whitelist: false,
            enforce_secure_profile: true,
            uses_authentication: true,
            log_ips: true,
            operator_user_permissions: LevelBasedPermissionSet::for_level(PermissionLevel::Owners),
        }
    }
}

pub struct PlayerList {
    players: Vec<PlayerHandle>,
    /// The same list shared with the levels (`ServerLevel::send_to_player`/`broadcast_to_all_players`).
    pub roster: crate::server::level::PlayerRoster,
    players_by_uuid: HashMap<Uuid, PlayerHandle>,
    pub bans: UserBanList,
    pub ip_bans: IpBanList,
    pub ops: ServerOpList,
    pub whitelist: UserWhiteList,
    stats: HashMap<Uuid, SharedStats>,
    advancements: HashMap<Uuid, PlayerAdvancementsFile>,
    player_io: PlayerDataStorage,
    /// `getWorldPath(PLAYER_STATS_DIR)` / `PLAYER_ADVANCEMENTS_DIR`.
    stats_dir: PathBuf,
    advancements_dir: PathBuf,
    pub(super) view_distance: i32,
    pub(super) simulation_distance: i32,
    send_all_player_info_in: i32,
    pub settings: PlayerListSettings,
    pub(super) notifications: Arc<dyn NotificationService>,
}

impl PlayerList {
    pub fn new(
        settings: PlayerListSettings,
        player_io: PlayerDataStorage,
        stats_dir: PathBuf,
        advancements_dir: PathBuf,
        notifications: Arc<dyn NotificationService>,
    ) -> Self {
        Self {
            players: Vec::new(),
            roster: Default::default(),
            players_by_uuid: HashMap::new(),
            whitelist: UserWhiteList::new(WHITELIST_FILE, notifications.clone()),
            ops: ServerOpList::new(OPLIST_FILE, notifications.clone()),
            bans: UserBanList::new(USERBANLIST_FILE, notifications.clone()),
            ip_bans: IpBanList::new(IPBANLIST_FILE, notifications.clone()),
            stats: HashMap::new(),
            advancements: HashMap::new(),
            player_io,
            stats_dir,
            advancements_dir,
            view_distance: 0,
            simulation_distance: 0,
            send_all_player_info_in: 0,
            settings,
            notifications,
        }
    }

    // ---------------------------------------------------------------------------------------------- accessors

    /// `getPlayers()`: the handles in join order (cheap clones, so callers may mutate the server while iterating).
    pub fn get_players(&self) -> Vec<PlayerHandle> {
        self.players.clone()
    }

    pub fn players(&self) -> &[PlayerHandle] {
        &self.players
    }

    pub fn get_players_by_uuid(&self) -> &HashMap<Uuid, PlayerHandle> {
        &self.players_by_uuid
    }

    pub fn get_player_count(&self) -> i32 {
        self.players.len() as i32
    }

    pub fn get_max_players(&self) -> i32 {
        self.settings.max_players
    }

    pub fn is_using_whitelist(&self) -> bool {
        self.settings.using_whitelist
    }

    pub fn get_view_distance(&self) -> i32 {
        self.view_distance
    }

    pub fn get_simulation_distance(&self) -> i32 {
        self.simulation_distance
    }

    /// `getPlayer(uuid)`.
    pub fn get_player(&self, uuid: &Uuid) -> Option<&PlayerHandle> {
        self.players_by_uuid.get(uuid)
    }

    /// `getPlayer(name)` / `getPlayerByName(name)`: case-insensitive, first in join order.
    pub fn get_player_by_name(&self, name: &str) -> Option<&PlayerHandle> {
        self.players.iter().find(|p| p.name().eq_ignore_ascii_case(name) || p.name().to_lowercase() == name.to_lowercase())
    }

    /// `getPlayerNamesArray()`.
    pub fn get_player_names_array(&self) -> Vec<String> {
        self.players.iter().map(|p| p.name().to_owned()).collect()
    }

    /// `getPlayersWithAddress(ip)`.
    pub fn get_players_with_address(&self, ip: &str) -> Vec<PlayerHandle> {
        self.players.iter().filter(|p| p.ip_address == ip).cloned().collect()
    }

    pub fn get_white_list_names(&self) -> Vec<String> {
        self.whitelist.get_user_list()
    }

    pub fn get_op_names(&self) -> Vec<String> {
        self.ops.get_user_list()
    }

    // ---------------------------------------------------------------------------------------------- permissions

    /// `isOp(nameAndId)` (dedicated: `isSingleplayerOwner` is always false).
    pub fn is_op(&self, name_and_id: &NameAndId) -> bool {
        self.ops.contains(name_and_id)
    }

    /// `isWhiteListed` (`DedicatedPlayerList`).
    pub fn is_white_listed(&self, name_and_id: &NameAndId) -> bool {
        !self.settings.using_whitelist || self.is_op(name_and_id) || self.whitelist.is_white_listed(name_and_id)
    }

    /// `canBypassPlayerLimit` (`DedicatedPlayerList`).
    pub fn can_bypass_player_limit(&mut self, name_and_id: &NameAndId) -> bool {
        self.ops.can_bypass_player_limit(name_and_id)
    }

    /// `MinecraftServer.getProfilePermissions(nameAndId)`.
    pub fn get_profile_permissions(&mut self, name_and_id: &NameAndId) -> LevelBasedPermissionSet {
        if self.is_op(name_and_id) {
            let operator = self.settings.operator_user_permissions;
            self.ops.get(name_and_id).map_or(operator, ServerOpListEntry::permissions)
        } else {
            LevelBasedPermissionSet::for_level(PermissionLevel::All)
        }
    }

    // ---------------------------------------------------------------------------------------------- login checks

    /// `canPlayerLogin(address, nameAndId)`: the disconnect reason, or `None` when the player may join.
    pub fn can_player_login(&mut self, address: &SocketAddr, name_and_id: &NameAndId) -> Option<Component> {
        if self.bans.is_banned(name_and_id) {
            let ban = self.bans.get(name_and_id)?;
            let mut reason = Component::translatable("multiplayer.disconnect.banned.reason", vec![ban.get_reason_message().into()]);
            if let Some(expires) = &ban.expires {
                reason = reason.append(Component::translatable("multiplayer.disconnect.banned.expiration", vec![format_ban_date(expires).into()]));
            }
            Some(reason)
        } else if !self.is_white_listed(name_and_id) {
            Some(Component::translatable("multiplayer.disconnect.not_whitelisted", vec![]))
        } else if self.ip_bans.is_banned_address(address) {
            let ban = self.ip_bans.get_address(address)?;
            let mut reason = Component::translatable("multiplayer.disconnect.banned_ip.reason", vec![ban.get_reason_message().into()]);
            if let Some(expires) = &ban.expires {
                reason = reason.append(Component::translatable("multiplayer.disconnect.banned_ip.expiration", vec![format_ban_date(expires).into()]));
            }
            Some(reason)
        } else if self.players.len() as i32 >= self.get_max_players() && !self.can_bypass_player_limit(name_and_id) {
            Some(Component::translatable("multiplayer.disconnect.server_full", vec![]))
        } else {
            None
        }
    }

    /// `disconnectAllPlayersWithProfile(playerId)`: kicks every connection of that profile with
    /// `multiplayer.disconnect.duplicate_login`; `true` if there was one.
    pub fn disconnect_all_players_with_profile(&self, player_id: &Uuid) -> bool {
        let mut dupes: Vec<&PlayerHandle> = self.players.iter().filter(|p| &p.uuid == player_id).collect();
        if let Some(by_uuid) = self.players_by_uuid.get(player_id)
            && !dupes.iter().any(|p| p.same_player(by_uuid))
        {
            dupes.push(by_uuid);
        }
        for player in &dupes {
            player.disconnect(&duplicate_login_disconnect_message());
        }
        !dupes.is_empty()
    }

    // ---------------------------------------------------------------------------------------------- membership

    /// The list part of `placeNewPlayer`: `players.add` + `playersByUUID.put`.
    pub fn add_player(&mut self, handle: PlayerHandle) {
        self.players_by_uuid.insert(handle.uuid, handle.clone());
        self.roster.add(handle.clone());
        self.players.push(handle);
    }

    /// The list part of `respawn`: `players.remove(serverPlayer)` (by identity).
    pub fn remove_from_players(&mut self, handle: &PlayerHandle) {
        self.roster.remove(handle);
        self.players.retain(|p| !p.same_player(handle));
    }

    /// The list part of `remove(player)` after the entity work: `players.remove`, then — if the by-uuid slot still
    /// belongs to this player (a duplicate login may have replaced it) — drop it, its stats/advancements and notify.
    pub fn remove_player(&mut self, handle: &PlayerHandle) {
        self.remove_from_players(handle);
        if self.players_by_uuid.get(&handle.uuid).is_some_and(|p| p.same_player(handle)) {
            self.players_by_uuid.remove(&handle.uuid);
            self.stats.remove(&handle.uuid);
            self.advancements.remove(&handle.uuid);
            self.notifications.player_left(&handle.name_and_id());
        }
    }

    // ---------------------------------------------------------------------------------------------- player files

    /// `loadPlayerData(nameAndId)` (dedicated: never a singleplayer owner).
    pub fn load_player_data(&self, name_and_id: &NameAndId) -> Option<CompoundTag> {
        self.player_io.load(&name_and_id.id, &name_and_id.name)
    }

    /// `playerIo.save(player)` for an already serialized player compound (failures are logged like vanilla:
    /// `Failed to save player data for {}`).
    pub fn save_player_data(&self, handle: &PlayerHandle, data: CompoundTag) {
        if let Err(e) = self.player_io.save(&handle.uuid, data) {
            log::warn!("Failed to save player data for {}: {e}", handle.name());
        }
    }

    /// `save(player)` minus the entity part: the stats and advancements files.
    pub fn save_player_files(&mut self, handle: &PlayerHandle) {
        if let Some(stats) = self.stats.get(&handle.uuid) {
            stats.lock().save();
        }
        if let Some(advancements) = self.advancements.get(&handle.uuid) {
            advancements.save();
        }
    }

    /// `getPlayerStats(player)`.
    pub fn get_player_stats(&mut self, handle: &PlayerHandle) -> SharedStats {
        let stats_dir = &self.stats_dir;
        self.stats
            .entry(handle.uuid)
            .or_insert_with(|| SharedStats::new(ServerStatsCounter::new(locate_stats_file(stats_dir, handle.uuid, handle.name()))))
            .clone()
    }

    /// `getPlayerAdvancements(player)`.
    pub fn get_player_advancements(&mut self, handle: &PlayerHandle) -> &mut PlayerAdvancementsFile {
        let dir = &self.advancements_dir;
        self.advancements.entry(handle.uuid).or_insert_with(|| PlayerAdvancementsFile::new(dir.join(format!("{}.json", handle.uuid))))
    }

    // ---------------------------------------------------------------------------------------------- tick / ops

    /// `tick()`: every 600 ticks send everybody's latency to everybody.
    fn tick_state(&mut self) -> bool {
        self.send_all_player_info_in += 1;
        if self.send_all_player_info_in > SEND_PLAYER_INFO_INTERVAL {
            self.send_all_player_info_in = 0;
            true
        } else {
            false
        }
    }

    /// `op(nameAndId, permissions, canBypassPlayerLimit)` list part; the caller sends the permission level.
    pub fn op_entry(&mut self, name_and_id: NameAndId, permissions: Option<LevelBasedPermissionSet>, can_bypass_player_limit: Option<bool>) {
        let permissions = permissions.unwrap_or(self.settings.operator_user_permissions);
        let bypass = can_bypass_player_limit.unwrap_or_else(|| self.ops.can_bypass_player_limit(&name_and_id));
        self.ops.add(ServerOpListEntry::new(name_and_id, permissions, bypass));
    }
}

/// `BAN_DATE_FORMAT`: `yyyy-MM-dd 'at' HH:mm:ss z` in the default time zone (`z`: see [`short_zone_name`]).
pub fn format_ban_date(date: &chrono::DateTime<chrono::Utc>) -> String {
    let Some(tm) = local_time(date.timestamp()) else { return date.format("%Y-%m-%d at %H:%M:%S UTC").to_string() };
    let offset = tm.tm_gmtoff;
    let zone = short_zone_name(offset, &tm);
    format!("{:04}-{:02}-{:02} at {:02}:{:02}:{:02} {zone}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// `localtime_r` (honours the process `TZ`, which chrono's cached `Local` does not re-read).
fn local_time(epoch_seconds: i64) -> Option<libc::tm> {
    // SAFETY: `localtime_r` fills the zeroed `tm` for a valid `time_t` and returns null on failure.
    unsafe {
        let t: libc::time_t = epoch_seconds as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        (!libc::localtime_r(&t, &mut tm).is_null()).then_some(tm)
    }
}

/// `z` of `SimpleDateFormat(.., Locale.ROOT)`: ROOT has no localized zone names, so Java prints `UTC`/`GMT` for a zero
/// offset and `GMT+hh:mm` otherwise (never `CET`).
fn short_zone_name(offset: libc::c_long, tm: &libc::tm) -> String {
    if offset == 0 {
        // SAFETY: `tm_zone` is null or a NUL-terminated string owned by libc's tz database; it is read immediately.
        let zone = unsafe { (!tm.tm_zone.is_null()).then(|| std::ffi::CStr::from_ptr(tm.tm_zone).to_string_lossy().into_owned()) };
        return if zone.as_deref() == Some("UTC") { "UTC" } else { "GMT" }.to_owned();
    }
    let (sign, abs) = if offset < 0 { ('-', -offset) } else { ('+', offset) };
    format!("GMT{sign}{:02}:{:02}", abs / 3600, abs % 3600 / 60)
}

impl PlayerList {
    /// `broadcastAll(packet)`.
    pub fn broadcast_all<P: Packet>(&self, packet: &P) {
        for player in &self.players {
            player.connection.send(packet);
        }
    }

    /// `broadcastAll(packet, dimension)`.
    pub fn broadcast_all_in_dimension<P: Packet>(&self, packet: &P, dimension: &DimensionKey) {
        for player in &self.players {
            if &player.dimension() == dimension {
                player.connection.send(packet);
            }
        }
    }

    /// `broadcast(except, x, y, z, range, dimension, packet)`.
    pub fn broadcast<P: Packet>(&self, except: Option<&PlayerHandle>, x: f64, y: f64, z: f64, range: f64, dimension: &DimensionKey, packet: &P) {
        for player in &self.players {
            if except.is_some_and(|e| e.same_player(player)) || &player.dimension() != dimension {
                continue;
            }
            let Vec3 { x: px, y: py, z: pz } = player.position();
            let (xd, yd, zd) = (x - px, y - py, z - pz);
            if xd * xd + yd * yd + zd * zd < range * range {
                player.connection.send(packet);
            }
        }
    }

    /// `tick()`.
    pub fn tick(&mut self) {
        if self.tick_state() {
            let packet = super::player_handle::create_info_update([ClientboundPlayerInfoUpdateAction::UpdateLatency].into_iter().collect::<EnumSet<_>>(), &self.players);
            self.broadcast_all(&packet);
        }
    }

    /// `broadcastSystemMessage(message, overlay)`.
    pub fn broadcast_system_message(&self, message: &Component, overlay: bool) {
        self.broadcast_system_message_with(message, |_| Some(message.clone()), overlay);
    }

    /// `broadcastSystemMessage(message, playerMessages, overlay)`; `server.sendSystemMessage` logs to the console.
    pub fn broadcast_system_message_with(
        &self,
        message: &Component,
        player_messages: impl Fn(&PlayerHandle) -> Option<Component>,
        overlay: bool,
    ) {
        log::info!("System chat: {}", message.get_string());
        for player in &self.players {
            if let Some(player_message) = player_messages(player) {
                player.send_system_message(&player_message, overlay);
            }
        }
    }

    /// `broadcastChatMessage(message, isFiltered, senderPlayer, chatType)`.
    pub fn broadcast_chat_message(
        &self,
        message: &PlayerChatMessage,
        is_filtered: impl Fn(&PlayerHandle) -> bool,
        sender_player: Option<&PlayerHandle>,
        chat_type: &Bound,
    ) {
        let trusted = verify_chat_trusted(message);
        log_chat_message(&message.decorated_content(), chat_type, if trusted { None } else { Some("Not Secure") });
        let tracked = OutgoingChatMessage::create(message);
        let mut was_fully_filtered = false;
        for player in &self.players {
            let filtered = is_filtered(player);
            player.send_chat_message(&tracked, filtered, chat_type);
            was_fully_filtered |= filtered && message.is_fully_filtered();
        }
        if was_fully_filtered && let Some(sender) = sender_player {
            sender.send_system_message(&chat_filtered_full(), false);
        }
    }

    /// `broadcastChatMessage(message, sender, chatType)` (filter predicate `sender.shouldFilterMessageTo`).
    pub fn broadcast_chat_message_from_player(&self, message: &PlayerChatMessage, sender: &PlayerHandle, chat_type: &Bound) {
        self.broadcast_chat_message(message, |receiver| sender.should_filter_message_to(receiver), Some(sender), chat_type);
    }

    pub fn broadcast_system_to_team(&self, sender: &PlayerHandle, team_members: &[String], message: &Component) {
        for name in team_members {
            if let Some(team_player) = self.get_player_by_name(name)
                && !team_player.same_player(sender)
            {
                team_player.send_system_message(message, false);
            }
        }
    }

    pub fn broadcast_system_to_all_except_team(
        &self,
        sender_team: Option<&str>,
        team_of: impl Fn(&PlayerHandle) -> Option<String>,
        message: &Component,
    ) {
        match sender_team {
            None => self.broadcast_system_message(message, false),
            Some(team) => {
                for target in &self.players {
                    if team_of(target).as_deref() != Some(team) {
                        target.send_system_message(message, false);
                    }
                }
            }
        }
    }

    /// `removeAll()`: kick everybody with `multiplayer.disconnect.server_shutdown`.
    pub fn remove_all(&self) {
        for player in &self.players {
            player.disconnect(&Component::translatable("multiplayer.disconnect.server_shutdown", vec![]));
        }
    }

    /// `broadcastAll(new ClientboundPlayerInfoRemovePacket(List.of(uuid)))` of `remove`.
    pub fn broadcast_player_info_remove(&self, uuid: Uuid) {
        self.broadcast_all(&ClientboundPlayerInfoRemovePacket { profile_ids: vec![uuid] });
    }
}

fn verify_chat_trusted(message: &PlayerChatMessage) -> bool {
    message.has_signature() && !message.has_expired_server(instant_now())
}

/// `MinecraftServer.logChatMessage`.
pub fn log_chat_message(message: &Component, chat_type: &Bound, tag: Option<&str>) {
    let decorated = chat_type.decorate(message).get_string();
    match tag {
        Some(tag) => log::info!("[{tag}] {decorated}"),
        None => log::info!("{decorated}"),
    }
}

/// `broadcastSystemToTeam(player, message)`/`broadcastSystemToAllExceptTeam` need the scoreboard's team of each player;
/// they take the team's member names (`team.getPlayers()`), resolving them like vanilla (`getPlayerByName`).
/// `broadcastSystemToAllExceptTeam(player, message)`: `team_of` gives each player's team name (`None` = no team);
/// the sender's own team is `sender_team`.
/// The `ClientboundEntityEventPacket` id `sendPlayerPermissionLevel` sends for a permission level (24..=28).
pub fn permission_level_event_id(permissions: LevelBasedPermissionSet) -> i8 {
    match permissions.level() {
        PermissionLevel::All => 24,
        PermissionLevel::Moderators => 25,
        PermissionLevel::Gamemasters => 26,
        PermissionLevel::Admins => 27,
        PermissionLevel::Owners => 28,
    }
}

/// `removeAll()`: kick everybody with `multiplayer.disconnect.server_shutdown`.
/// `new ClientboundPlayerInfoRemovePacket(List.of(uuid))` broadcast of `remove`.
/// Plain-text join message pieces (`multiplayer.player.joined[.renamed]`, yellow).
pub fn join_message(display_name: Component, old_name: Option<&str>) -> Component {
    let message = match old_name {
        None => Component::translatable("multiplayer.player.joined", vec![TranslatableArg::Component(display_name)]),
        Some(old) => Component::translatable("multiplayer.player.joined.renamed", vec![TranslatableArg::Component(display_name), old.into()]),
    };
    message.with_format(ChatFormatting::Yellow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::stats::Stats;
    use crate::server::chat::chat_type;
    use crate::server::notifications::EmptyNotificationService;
    use crate::server::players::player_handle::test_support::handle;
    use mc_protocol::packets::common::ChatVisibility;
    use mc_protocol::packets::play::{
        ClientboundDisguisedChatPacket, ClientboundPlayerChatPacket, ClientboundPlayerInfoRemovePacket as Remove, ClientboundSetTimePacket,
        ClientboundSystemChatPacket,
    };

    fn list(dir: &std::path::Path) -> PlayerList {
        let mut list = PlayerList::new(
            PlayerListSettings { max_players: 2, ..Default::default() },
            PlayerDataStorage::new(dir.join("players/data")),
            dir.join("players/stats"),
            dir.join("players/advancements"),
            Arc::new(EmptyNotificationService),
        );
        // keep the user lists inside the temp dir
        list.whitelist = UserWhiteList::new(dir.join("whitelist.json"), Arc::new(EmptyNotificationService));
        list.ops = ServerOpList::new(dir.join("ops.json"), Arc::new(EmptyNotificationService));
        list.bans = UserBanList::new(dir.join("banned-players.json"), Arc::new(EmptyNotificationService));
        list.ip_bans = IpBanList::new(dir.join("banned-ips.json"), Arc::new(EmptyNotificationService));
        list
    }

    fn addr() -> SocketAddr {
        "10.1.2.3:5555".parse().unwrap()
    }

    fn id_of<P: Packet>() -> Option<i32> {
        Some(P::ID)
    }

    fn bound(key: &str, name: &str) -> Bound {
        let (_, ct) = chat_type::bootstrap().into_iter().find(|(k, _)| *k == key).unwrap();
        Bound { chat_type_id: 0, chat_type: Arc::new(ct), name: Component::literal(name), target_name: None }
    }

    #[test]
    fn login_checks_in_vanilla_order() {
        use super::super::ban_list_entry::{IpBanListEntry, UserBanListEntry};
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let steve = NameAndId::create_offline("Steve");
        assert!(list.can_player_login(&addr(), &steve).is_none());

        // banned (with reason, no expiry)
        list.bans.add(UserBanListEntry::new(Some(steve.clone()), None, Some("op"), None, Some("Griefing")));
        let reason = list.can_player_login(&addr(), &steve).unwrap();
        assert_eq!(reason.get_string(), "You are banned from this server.\nReason: Griefing");
        list.bans.remove(&steve);

        // whitelist: ops bypass it, others are rejected
        list.settings.using_whitelist = true;
        assert_eq!(list.can_player_login(&addr(), &steve).unwrap().get_string(), "You are not white-listed on this server!");
        list.op_entry(steve.clone(), None, None);
        assert!(list.can_player_login(&addr(), &steve).is_none());
        list.ops.remove(&steve);
        list.settings.using_whitelist = false;

        // ip ban
        list.ip_bans.add(IpBanListEntry::new(Some("10.1.2.3".to_owned()), None, None, None, None));
        assert_eq!(
            list.can_player_login(&addr(), &steve).unwrap().get_string(),
            "Your IP address is banned from this server.\nReason: Banned by an operator."
        );
        list.ip_bans.remove("10.1.2.3");

        // full
        for (i, n) in ["A", "B"].into_iter().enumerate() {
            list.add_player(handle(i as i32 + 1, n, "minecraft:overworld").0);
        }
        assert_eq!(list.can_player_login(&addr(), &steve).unwrap().get_string(), "The server is full!");
        list.op_entry(steve.clone(), None, Some(true));
        assert!(list.can_player_login(&addr(), &steve).is_none(), "ops with bypassesPlayerLimit may join a full server");
    }

    #[test]
    fn expiring_ban_appends_the_date() {
        use super::super::ban_list_entry::UserBanListEntry;
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let steve = NameAndId::create_offline("Steve");
        let expires = chrono::Utc::now() + chrono::Duration::days(1);
        list.bans.add(UserBanListEntry::new(Some(steve.clone()), None, None, Some(expires), Some("x")));
        let text = list.can_player_login(&addr(), &steve).unwrap().get_string();
        assert!(text.starts_with("You are banned from this server.\nReason: x\nYour ban will be removed on 20"), "{text}");
        assert!(text.contains(" at "), "{text}");
    }

    #[test]
    fn broadcasts_reach_every_handle_and_respect_dimension_and_range() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let (a, mut wa) = handle(1, "A", "minecraft:overworld");
        let (b, mut wb) = handle(2, "B", "minecraft:overworld");
        let (c, mut wc) = handle(3, "C", "minecraft:the_nether");
        a.shared.lock().position = Vec3::new(0.0, 64.0, 0.0);
        b.shared.lock().position = Vec3::new(10.0, 64.0, 0.0);
        c.shared.lock().position = Vec3::new(1.0, 64.0, 0.0);
        for h in [&a, &b, &c] {
            list.add_player(h.clone());
        }
        let packet = ClientboundSetTimePacket { game_time: 1, clock_updates: vec![] };
        list.broadcast_all(&packet);
        assert_eq!(wa.ids(), [id_of::<ClientboundSetTimePacket>()]);
        assert_eq!(wb.ids().len(), 1);
        assert_eq!(wc.ids().len(), 1);

        let overworld = DimensionKey::try_parse("minecraft:overworld").unwrap();
        list.broadcast_all_in_dimension(&packet, &overworld);
        assert_eq!((wa.ids().len(), wb.ids().len(), wc.ids().len()), (1, 1, 0));

        // `player != except`, same dimension, strictly inside the range
        list.broadcast(Some(&a), 0.0, 64.0, 0.0, 10.0, &overworld, &packet);
        assert_eq!((wa.ids().len(), wb.ids().len(), wc.ids().len()), (0, 0, 0), "B is exactly at range: not inside");
        list.broadcast(None, 0.0, 64.0, 0.0, 10.5, &overworld, &packet);
        assert_eq!((wa.ids().len(), wb.ids().len(), wc.ids().len()), (1, 1, 0));
    }

    #[test]
    fn system_messages_honour_chat_visibility_but_overlay_always_shows() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let (a, mut wa) = handle(1, "A", "minecraft:overworld");
        let (b, mut wb) = handle(2, "B", "minecraft:overworld");
        b.shared.lock().chat_visibility = ChatVisibility::Hidden;
        list.add_player(a);
        list.add_player(b);
        let msg = Component::literal("hi");
        list.broadcast_system_message(&msg, false);
        assert_eq!((wa.ids().len(), wb.ids().len()), (1, 0));
        list.broadcast_system_message(&msg, true);
        assert_eq!((wa.ids(), wb.ids()), (vec![id_of::<ClientboundSystemChatPacket>()], vec![id_of::<ClientboundSystemChatPacket>()]));
    }

    #[test]
    fn chat_goes_signed_to_full_visibility_players_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let (a, mut wa) = handle(1, "A", "minecraft:overworld");
        let (b, mut wb) = handle(2, "B", "minecraft:overworld");
        b.shared.lock().chat_visibility = ChatVisibility::System;
        list.add_player(a.clone());
        list.add_player(b);
        let message = PlayerChatMessage::unsigned(a.uuid, "hello");
        list.broadcast_chat_message_from_player(&message, &a, &bound(chat_type::CHAT, "A"));
        assert_eq!(wa.ids(), [id_of::<ClientboundPlayerChatPacket>()]);
        assert!(wb.ids().is_empty());
        // system-sender messages are disguised
        let system = PlayerChatMessage::system("announce");
        list.broadcast_chat_message(&system, |_| false, None, &bound(chat_type::SAY_COMMAND, "Server"));
        assert_eq!(wa.ids(), [id_of::<ClientboundDisguisedChatPacket>()]);
    }

    #[test]
    fn fully_filtered_message_is_dropped_and_sender_told() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let (a, mut wa) = handle(1, "A", "minecraft:overworld");
        let (b, mut wb) = handle(2, "B", "minecraft:overworld");
        b.shared.lock().text_filtering_enabled = true;
        list.add_player(a.clone());
        list.add_player(b);
        let mut message = PlayerChatMessage::unsigned(a.uuid, "bad");
        message.filter_mask = mc_protocol::packets::play::FilterMask::FullyFiltered;
        list.broadcast_chat_message_from_player(&message, &a, &bound(chat_type::CHAT, "A"));
        assert!(wb.ids().is_empty(), "filtered for B and fully masked: not sent");
        // A receives its own message (never filtered for the sender), then `chat.filtered_full`
        assert_eq!(wa.ids(), [id_of::<ClientboundPlayerChatPacket>(), id_of::<ClientboundSystemChatPacket>()]);
    }

    #[test]
    fn duplicate_login_kicks_the_old_connection_and_remove_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = list(dir.path());
        let (a, mut wa) = handle(1, "A", "minecraft:overworld");
        list.add_player(a.clone());
        assert!(list.disconnect_all_players_with_profile(&a.uuid));
        // `Outbound::Disconnect` is the disconnect packet of the current protocol followed by the close, as one message
        assert_eq!(wa.ids(), [None], "disconnect packet + close");
        assert!(!list.disconnect_all_players_with_profile(&Uuid::from_u128(1)));

        list.get_player_stats(&a).lock().increment(Stats::LEAVE_GAME, 1);
        list.save_player_files(&a);
        assert!(dir.path().join("players/stats").join(format!("{}.json", a.uuid)).exists());
        list.remove_player(&a);
        assert_eq!(list.get_player_count(), 0);
        assert!(list.get_player(&a.uuid).is_none());

        let (b, mut wb) = handle(2, "B", "minecraft:overworld");
        list.add_player(b.clone());
        list.broadcast_player_info_remove(b.uuid);
        assert_eq!(wb.ids(), [id_of::<Remove>()]);
    }

    #[test]
    fn permission_level_events() {
        assert_eq!(permission_level_event_id(LevelBasedPermissionSet::ALL), 24);
        assert_eq!(permission_level_event_id(LevelBasedPermissionSet::for_level(PermissionLevel::Owners)), 28);
    }
}
