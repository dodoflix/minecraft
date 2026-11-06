//! `IncomingRpcMethods` and the `*Service` classes of `net.minecraft.server.jsonrpc.methods`, plus the request
//! handling of `Connection` (`handleJsonObject`, batches, error responses).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_text::Component;
use uuid::Uuid;

use super::api::{ClientInfo, ManagementApi, ManagementPlayer, SettingKey, SettingValue};
use super::dto::*;
use crate::server::permissions::PermissionLevel;
use crate::server::players::gson::{self, JsonElement, JsonObject};
use crate::server::players::{NameAndId, UserWhiteListEntry};
use mc_world::level_storage::{Difficulty, GameType};

/// The `JsonRPCErrors` codes.
#[derive(Clone, Copy)]
pub enum JsonRpcError {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
}

impl JsonRpcError {
    fn code_and_message(self) -> (i32, &'static str) {
        match self {
            JsonRpcError::ParseError => (-32700, "Parse error"),
            JsonRpcError::InvalidRequest => (-32600, "Invalid Request"),
            JsonRpcError::MethodNotFound => (-32601, "Method not found"),
            JsonRpcError::InvalidParams => (-32602, "Invalid params"),
            JsonRpcError::InternalError => (-32603, "Internal error"),
        }
    }

    /// `JsonRPCUtils.createError`: `data` is only written when non-blank.
    pub fn create(self, id: &JsonElement, data: Option<&str>) -> JsonElement {
        let (code, message) = self.code_and_message();
        let mut error = JsonObject::new();
        error.add_property_int("code", code as i64);
        error.add_property_str("message", Some(message));
        if let Some(d) = data.filter(|d| !d.trim().is_empty()) {
            error.add_property_str("data", Some(d));
        }
        let mut response = JsonObject::new();
        response.add_property_str("jsonrpc", Some("2.0"));
        response.add("id", id.clone());
        response.add("error", JsonElement::Object(error));
        JsonElement::Object(response)
    }
}

/// The exceptions an incoming method can throw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    InvalidParameter(String),
    Encode(String),
    InvalidRequest(String),
    MethodNotFound(String),
    /// Any other exception: logged, answered with a bare `Internal error`.
    Internal(String),
}

impl From<DecodeError> for RpcError {
    fn from(e: DecodeError) -> Self {
        RpcError::InvalidParameter(e.0)
    }
}

type RpcResult = Result<JsonElement, RpcError>;

/// `JsonRpcLogger.log`.
pub fn rpc_log(client: ClientInfo, message: impl std::fmt::Display) {
    log::info!("RPC Connection #{}: {message}", client.connection_id);
}

struct Ctx {
    api: Arc<dyn ManagementApi>,
    client: ClientInfo,
}

impl Ctx {
    fn log(&self, message: impl std::fmt::Display) {
        rpc_log(self.client, message);
    }

    /// `MinecraftPlayerListService.getUser(id, name)`.
    fn get_user(&self, dto: &PlayerDto) -> Option<NameAndId> {
        if let Some(id) = dto.id {
            return self
                .api
                .cached_user_by_id(id)
                .or_else(|| self.api.fetch_user_by_id(id));
        }
        dto.name
            .as_deref()
            .and_then(|n| self.api.fetch_user_by_name(n))
    }

    /// `MinecraftPlayerListService.getPlayer(Optional id, Optional name)`.
    fn get_player(&self, dto: &PlayerDto) -> Option<ManagementPlayer> {
        match (&dto.id, &dto.name) {
            (Some(id), _) => self.api.player_by_id(*id),
            (None, Some(name)) => self.api.player_by_name(name),
            _ => None,
        }
    }

    fn players_dto(&self) -> Vec<PlayerDto> {
        self.api
            .players()
            .iter()
            .map(|p| PlayerDto::from_name_and_id(&p.profile))
            .collect()
    }
}

fn list(items: Vec<JsonElement>) -> JsonElement {
    JsonElement::Array(items)
}

fn bool_e(b: bool) -> JsonElement {
    JsonElement::Bool(b)
}

fn int_e(i: i64) -> JsonElement {
    JsonElement::Number(i.to_string())
}

fn dedup<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// Guava `InetAddresses.isInetAddress`: an IPv4 or IPv6 literal.
fn is_inet_address(s: &str) -> bool {
    s.parse::<IpAddr>().is_ok()
}

// ---- allowlist ---------------------------------------------------------------------------------------------------

fn allowlist_get(c: &Ctx) -> JsonElement {
    let users: Vec<NameAndId> = c
        .api
        .allowlist_entries()
        .iter()
        .filter_map(|e| crate::server::players::StoredUserEntry::user(e).cloned())
        .collect();
    encode_player_list(&users)
}

fn allowlist_add_user(c: &Ctx, user: NameAndId) {
    c.log(format!("Add player '{user}' to allowlist"));
    c.api.allowlist_add(UserWhiteListEntry::new(user));
}

fn allowlist_remove_user(c: &Ctx, user: &NameAndId) {
    c.log(format!("Remove player '{user}' from allowlist"));
    c.api.allowlist_remove(user);
}

fn kick_unlisted(c: &Ctx) {
    c.log("Kick unlisted players");
    c.api.kick_unlisted_players();
}

fn allowlist_set(c: &Ctx, p: &JsonElement) -> RpcResult {
    let dtos = decode_list_of(p, PlayerDto::decode)?;
    let fin: Vec<NameAndId> = dedup(dtos.iter().filter_map(|d| c.get_user(d)).collect());
    let current: Vec<NameAndId> = dedup(
        c.api
            .allowlist_entries()
            .iter()
            .filter_map(|e| crate::server::players::StoredUserEntry::user(e).cloned())
            .collect(),
    );
    for user in current.iter().filter(|u| !fin.contains(u)) {
        allowlist_remove_user(c, user);
    }
    for user in fin.iter().filter(|u| !current.contains(u)) {
        allowlist_add_user(c, user.clone());
    }
    kick_unlisted(c);
    Ok(allowlist_get(c))
}

// ---- bans -----------------------------------------------------------------------------------------------------------

fn bans_get(c: &Ctx) -> JsonElement {
    list(
        c.api
            .user_ban_entries()
            .iter()
            .filter_map(UserBanDto::from_entry)
            .map(|d| d.encode())
            .collect(),
    )
}

fn add_user_ban(c: &Ctx, ban: &UserBan) {
    let entry = ban.to_entry();
    c.log(format!(
        "Add player '{}' to banlist. Reason: '{}'",
        plain(&entry.get_display_name()),
        plain(&entry.get_reason_message())
    ));
    c.api.add_user_ban(entry);
}

fn plain(component: &Component) -> String {
    component.get_string()
}

fn ban_disconnect(c: &Ctx, ban: &UserBan) {
    if c.api.player_by_id(ban.player.id).is_some() {
        c.api.disconnect_player(
            ban.player.id,
            Component::translatable("multiplayer.disconnect.banned", vec![]),
        );
    }
}

fn bans_add(c: &Ctx, p: &JsonElement) -> RpcResult {
    let dtos = decode_list_of(p, UserBanDto::decode)?;
    let resolved: Vec<UserBan> = dtos
        .iter()
        .filter_map(|d| c.get_user(&d.player).map(|u| UserBan::from_dto(d, u)))
        .collect();
    for ban in &resolved {
        add_user_ban(c, ban);
        ban_disconnect(c, ban);
    }
    Ok(bans_get(c))
}

fn bans_remove(c: &Ctx, p: &JsonElement) -> RpcResult {
    for dto in decode_list_of(p, PlayerDto::decode)? {
        if let Some(user) = c.get_user(&dto) {
            c.log(format!("Remove player '{user}' from banlist"));
            c.api.remove_user_ban(&user);
        }
    }
    Ok(bans_get(c))
}

fn bans_set(c: &Ctx, p: &JsonElement) -> RpcResult {
    let dtos = decode_list_of(p, UserBanDto::decode)?;
    let fin: Vec<UserBan> = dedup(
        dtos.iter()
            .filter_map(|d| c.get_user(&d.player).map(|u| UserBan::from_dto(d, u)))
            .collect(),
    );
    let current: Vec<UserBan> = dedup(
        c.api
            .user_ban_entries()
            .iter()
            .filter_map(UserBan::from_entry)
            .collect(),
    );
    for ban in current.iter().filter(|b| !fin.contains(b)) {
        c.log(format!("Remove player '{}' from banlist", ban.player));
        c.api.remove_user_ban(&ban.player);
    }
    for ban in fin.iter().filter(|b| !current.contains(b)) {
        add_user_ban(c, ban);
        ban_disconnect(c, ban);
    }
    Ok(bans_get(c))
}

fn ip_bans_get(c: &Ctx) -> JsonElement {
    list(
        c.api
            .ip_ban_entries()
            .iter()
            .filter_map(IpBanDto::from_entry)
            .map(|d| d.encode())
            .collect(),
    )
}

fn add_ip_ban(c: &Ctx, ban: &IpBan) {
    c.log(format!("Add ip '{}' to ban list", ban.ip));
    c.api.add_ip_ban(ban.to_entry());
}

fn ip_banned_kick(c: &Ctx, players: Vec<ManagementPlayer>) {
    for player in players {
        c.api.disconnect_player(
            player.profile.id,
            Component::translatable("multiplayer.disconnect.ip_banned", vec![]),
        );
    }
}

fn ip_bans_add(c: &Ctx, p: &JsonElement) -> RpcResult {
    let dtos = decode_list_of(p, IncomingIpBanDto::decode)?;
    let mut to_kick = Vec::new();
    for ban in &dtos {
        let ip_ban = match ban.ip.as_deref().filter(|ip| is_inet_address(ip)) {
            Some(ip) => Some(ban.to_ip_ban(ip.to_owned())),
            None => ban
                .player
                .as_ref()
                .and_then(|p| c.get_player(p))
                .map(|player| ban.to_ip_ban(player.ip_address)),
        };
        if let Some(ip_ban) = ip_ban {
            add_ip_ban(c, &ip_ban);
            to_kick.extend(c.api.players_with_address(&ip_ban.ip));
        }
    }
    ip_banned_kick(c, to_kick);
    Ok(ip_bans_get(c))
}

fn ip_bans_remove(c: &Ctx, p: &JsonElement) -> RpcResult {
    for ip in decode_list_of(p, decode_string)? {
        c.log(format!("Remove ip '{ip}' from ban list"));
        c.api.remove_ip_ban(&ip);
    }
    Ok(ip_bans_get(c))
}

fn ip_bans_set(c: &Ctx, p: &JsonElement) -> RpcResult {
    let dtos = decode_list_of(p, IpBanDto::decode)?;
    let fin: Vec<IpBan> = dedup(
        dtos.iter()
            .filter(|d| is_inet_address(&d.ip))
            .map(IpBan::from_dto)
            .collect(),
    );
    let current: Vec<IpBan> = dedup(
        c.api
            .ip_ban_entries()
            .iter()
            .filter_map(IpBan::from_entry)
            .collect(),
    );
    for ban in current.iter().filter(|b| !fin.contains(b)) {
        c.log(format!("Remove ip '{}' from ban list", ban.ip));
        c.api.remove_ip_ban(&ban.ip);
    }
    for ban in fin.iter().filter(|b| !current.contains(b)) {
        add_ip_ban(c, ban);
    }
    let mut to_kick = Vec::new();
    for ban in fin.iter().filter(|b| !current.contains(b)) {
        to_kick.extend(c.api.players_with_address(&ban.ip));
    }
    ip_banned_kick(c, to_kick);
    Ok(ip_bans_get(c))
}

// ---- operators ----------------------------------------------------------------------------------------------------

fn operators_get(c: &Ctx) -> JsonElement {
    list(
        c.api
            .operator_entries()
            .iter()
            .filter_map(OperatorDto::from_entry)
            .map(|d| d.encode())
            .collect(),
    )
}

#[derive(PartialEq, Eq, Clone)]
struct Op {
    user: NameAndId,
    level: Option<PermissionLevel>,
    bypass: Option<bool>,
}

fn op_user(c: &Ctx, op: &Op) {
    c.log(format!("Op '{}'", op.user));
    c.api.op(op.user.clone(), op.level, op.bypass);
}

fn deop_user(c: &Ctx, user: &NameAndId) {
    c.log(format!("Deop '{user}'"));
    c.api.deop(user);
}

fn resolve_ops(c: &Ctx, dtos: &[OperatorDto]) -> Vec<Op> {
    dtos.iter()
        .filter_map(|d| {
            c.get_user(&d.player).map(|user| Op {
                user,
                level: d.permission_level,
                bypass: d.bypasses_player_limit,
            })
        })
        .collect()
}

fn operators_add(c: &Ctx, p: &JsonElement) -> RpcResult {
    for op in resolve_ops(c, &decode_list_of(p, OperatorDto::decode)?) {
        op_user(c, &op);
    }
    Ok(operators_get(c))
}

fn operators_remove(c: &Ctx, p: &JsonElement) -> RpcResult {
    for dto in decode_list_of(p, PlayerDto::decode)? {
        if let Some(user) = c.get_user(&dto) {
            deop_user(c, &user);
        }
    }
    Ok(operators_get(c))
}

fn operators_set(c: &Ctx, p: &JsonElement) -> RpcResult {
    let fin = dedup(resolve_ops(c, &decode_list_of(p, OperatorDto::decode)?));
    let current: Vec<Op> = dedup(
        c.api
            .operator_entries()
            .iter()
            .filter_map(|e| {
                let user = crate::server::players::StoredUserEntry::user(e)?.clone();
                Some(Op {
                    user,
                    level: Some(e.permissions().level()),
                    bypass: Some(e.get_bypasses_player_limit()),
                })
            })
            .collect(),
    );
    for op in current.iter().filter(|o| !fin.contains(o)) {
        deop_user(c, &op.user);
    }
    for op in fin.iter().filter(|o| !current.contains(o)) {
        op_user(c, op);
    }
    Ok(operators_get(c))
}

// ---- players ---------------------------------------------------------------------------------------------------------

fn players_kick(c: &Ctx, p: &JsonElement) -> RpcResult {
    let mut kicked = Vec::new();
    for kick in decode_list_of(p, KickDto::decode)? {
        let player = match (&kick.player.id, &kick.player.name) {
            (Some(id), _) => c.api.player_by_id(*id),
            (None, Some(name)) => c.api.player_by_name(name),
            _ => None,
        };
        if let Some(player) = player {
            c.api.remove_player(player.profile.id);
            c.log(format!("Remove player '{}'", player.profile.name));
            let reason = kick
                .message
                .as_ref()
                .and_then(|m| m.as_component())
                .unwrap_or_else(|| {
                    Component::translatable("multiplayer.disconnect.kicked", vec![])
                });
            c.api.disconnect_player(player.profile.id, reason);
            kicked.push(kick.player.encode());
        }
    }
    Ok(list(kicked))
}

// ---- server state ----------------------------------------------------------------------------------------------------

fn server_state(c: &Ctx) -> JsonElement {
    let version = c.api.version();
    if !c.api.is_ready() {
        return encode_server_state(false, &[], &version);
    }
    encode_server_state(true, &c.players_dto(), &version)
}

/// `ServerStateService.systemMessage`.
fn system_message(c: &Ctx, p: &JsonElement) -> RpcResult {
    let msg = SystemMessage::decode(p)?;
    let Some(component) = msg.message.as_component() else {
        return Ok(bool_e(false));
    };
    match &msg.receiving_players {
        Some(players) => {
            if players.is_empty() {
                return Ok(bool_e(false));
            }
            for dto in players {
                let player = match (&dto.id, &dto.name) {
                    (Some(id), _) => c.api.player_by_id(*id),
                    (None, Some(name)) => c.api.player_by_name(name),
                    _ => continue,
                };
                if let Some(player) = player {
                    c.api
                        .send_system_message(&[player.profile.id], component.clone(), msg.overlay);
                }
            }
        }
        None => {
            c.log(format!(
                "Broadcast system message (overlay: {}): '{}'",
                msg.overlay,
                plain(&component)
            ));
            c.api.broadcast_system_message(component, msg.overlay);
        }
    }
    Ok(bool_e(true))
}

// ---- server settings -----------------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Kind {
    Bool,
    Int,
    Str,
    Difficulty,
    GameType,
    PermissionLevel,
}

fn java_enum_name(v: &SettingValue) -> String {
    match v {
        SettingValue::Difficulty(d) => d.get_serialized_name().to_uppercase(),
        SettingValue::GameType(g) => g.get_name().to_uppercase(),
        other => display_setting(other),
    }
}

fn display_setting(v: &SettingValue) -> String {
    match v {
        SettingValue::Bool(b) => b.to_string(),
        SettingValue::Int(i) => i.to_string(),
        SettingValue::Str(s) => s.clone(),
        SettingValue::Difficulty(d) => d.get_serialized_name().to_uppercase(),
        SettingValue::GameType(g) => g.get_name().to_uppercase(),
        SettingValue::PermissionLevel(l) => l.name().to_owned(),
    }
}

fn encode_setting(v: &SettingValue) -> JsonElement {
    match v {
        SettingValue::Bool(b) => bool_e(*b),
        SettingValue::Int(i) => int_e(*i as i64),
        SettingValue::Str(s) => JsonElement::String(s.clone()),
        SettingValue::Difficulty(d) => JsonElement::String(d.get_serialized_name().to_owned()),
        SettingValue::GameType(g) => JsonElement::String(g.get_name().to_owned()),
        SettingValue::PermissionLevel(l) => int_e(l.id() as i64),
    }
}

fn decode_setting(kind: Kind, e: &JsonElement) -> Decoded<SettingValue> {
    Ok(match kind {
        Kind::Bool => SettingValue::Bool(decode_bool(e)?),
        Kind::Int => SettingValue::Int(decode_int(e)?),
        Kind::Str => SettingValue::Str(decode_string(e)?),
        Kind::Difficulty => {
            let s = decode_string(e)?;
            SettingValue::Difficulty(
                Difficulty::by_name(&s)
                    .ok_or_else(|| DecodeError(format!("Unknown element name:{s}")))?,
            )
        }
        Kind::GameType => {
            let s = decode_string(e)?;
            let game_type = [
                GameType::Survival,
                GameType::Creative,
                GameType::Adventure,
                GameType::Spectator,
            ]
            .into_iter()
            .find(|g| g.get_name() == s)
            .ok_or_else(|| DecodeError(format!("Unknown element name:{s}")))?;
            SettingValue::GameType(game_type)
        }
        Kind::PermissionLevel => {
            SettingValue::PermissionLevel(PermissionLevel::by_id(decode_int(e)?))
        }
    })
}

/// `(method path, setting, value kind, log template with {0}=old, {1}=new, also kick unlisted players afterwards)`.
const SETTINGS: &[(&str, SettingKey, Kind, &str, bool)] = &[
    (
        "autosave",
        SettingKey::AutoSave,
        Kind::Bool,
        "Update autosave from {0} to {1}",
        false,
    ),
    (
        "difficulty",
        SettingKey::Difficulty,
        Kind::Difficulty,
        "Update difficulty from '{0}' to '{1}'",
        false,
    ),
    (
        "enforce_allowlist",
        SettingKey::EnforceWhitelist,
        Kind::Bool,
        "Update enforce allowlist from {0} to {1}",
        true,
    ),
    (
        "use_allowlist",
        SettingKey::UsingWhitelist,
        Kind::Bool,
        "Update using allowlist from {0} to {1}",
        true,
    ),
    (
        "max_players",
        SettingKey::MaxPlayers,
        Kind::Int,
        "Update max players from {0} to {1}",
        false,
    ),
    (
        "pause_when_empty_seconds",
        SettingKey::PauseWhenEmptySeconds,
        Kind::Int,
        "Update pause when empty from {0} seconds to {1} seconds",
        false,
    ),
    (
        "player_idle_timeout",
        SettingKey::PlayerIdleTimeout,
        Kind::Int,
        "Update player idle timeout from {0} minutes to {1} minutes",
        false,
    ),
    (
        "allow_flight",
        SettingKey::AllowFlight,
        Kind::Bool,
        "Update allow flight from {0} to {1}",
        false,
    ),
    (
        "spawn_protection_radius",
        SettingKey::SpawnProtectionRadius,
        Kind::Int,
        "Update spawn protection radius from {0} to {1}",
        false,
    ),
    (
        "motd",
        SettingKey::Motd,
        Kind::Str,
        "Update MOTD from '{0}' to '{1}'",
        false,
    ),
    (
        "force_game_mode",
        SettingKey::ForceGameMode,
        Kind::Bool,
        "Update force game mode from {0} to {1}",
        false,
    ),
    (
        "game_mode",
        SettingKey::GameMode,
        Kind::GameType,
        "Update game mode from '{0}' to '{1}'",
        false,
    ),
    (
        "view_distance",
        SettingKey::ViewDistance,
        Kind::Int,
        "Update view distance from {0} to {1}",
        false,
    ),
    (
        "simulation_distance",
        SettingKey::SimulationDistance,
        Kind::Int,
        "Update simulation distance from {0} to {1}",
        false,
    ),
    (
        "accept_transfers",
        SettingKey::AcceptsTransfers,
        Kind::Bool,
        "Update accepts transfers from {0} to {1}",
        false,
    ),
    (
        "status_heartbeat_interval",
        SettingKey::StatusHeartbeatInterval,
        Kind::Int,
        "",
        false,
    ),
    (
        "operator_user_permission_level",
        SettingKey::OperatorUserPermissions,
        Kind::PermissionLevel,
        "Update operator user permission level from permission level: {0} to {1}",
        false,
    ),
    (
        "hide_online_players",
        SettingKey::HidesOnlinePlayers,
        Kind::Bool,
        "Update hides online players from {0} to {1}",
        false,
    ),
    (
        "status_replies",
        SettingKey::RepliesToStatus,
        Kind::Bool,
        "Update replies to status from {0} to {1}",
        false,
    ),
    (
        "entity_broadcast_range",
        SettingKey::EntityBroadcastRangePercentage,
        Kind::Int,
        "Update entity broadcast range percentage from {0}% to {1}%",
        false,
    ),
];

fn set_setting(
    c: &Ctx,
    key: SettingKey,
    template: &str,
    kick: bool,
    value: SettingValue,
) -> JsonElement {
    let old = c.api.get_setting(key);
    match key {
        SettingKey::StatusHeartbeatInterval => {
            let SettingValue::Int(seconds) = value else {
                unreachable!("kind is Int")
            };
            if c.api.set_status_heartbeat_interval(seconds) {
                c.log(format!(
                    "Updated status heartbeat interval from {} to {seconds}",
                    display_setting(&old)
                ));
            } else {
                c.log(format!(
                    "The heartbeat interval was not updated to the new value of {seconds} seconds as the json rpc server is null or a pre-existing heartbeat failed to get canceled"
                ));
            }
        }
        _ => {
            let new_text = java_enum_name(&value);
            let old_text = java_enum_name(&old);
            c.log(template.replace("{0}", &old_text).replace("{1}", &new_text));
            c.api.set_setting(key, value);
            if kick {
                c.api.kick_unlisted_players();
            }
        }
    }
    encode_setting(&c.api.get_setting(key))
}

// ---- game rules ----------------------------------------------------------------------------------------------------------

fn gamerules_get(c: &Ctx) -> JsonElement {
    list(
        c.api
            .game_rules()
            .iter()
            .map(encode_typed_game_rule)
            .collect(),
    )
}

fn gamerules_update(c: &Ctx, p: &JsonElement) -> RpcResult {
    let (key, value) = decode_untyped_game_rule(p)?;
    let old = c
        .api
        .game_rules()
        .into_iter()
        .find(|r| r.id == key || r.id.strip_prefix("minecraft:") == Some(key.as_str()));
    let updated = c
        .api
        .update_game_rule(&key, value)
        .map_err(RpcError::InvalidParameter)?;
    let show = |v: super::api::GameRuleValue| match v {
        super::api::GameRuleValue::Bool(b) => b.to_string(),
        super::api::GameRuleValue::Int(i) => i.to_string(),
    };
    c.log(format!(
        "Game rule '{}' updated from '{}' to '{}'",
        updated.id,
        old.map(|o| show(o.value)).unwrap_or_default(),
        show(updated.value)
    ));
    Ok(encode_typed_game_rule(&updated))
}

// ---- table -----------------------------------------------------------------------------------------------------------------

type Handler = Box<dyn Fn(&Ctx, &JsonElement) -> RpcResult + Send + Sync>;

struct Method {
    /// The single parameter's name (`None` for parameterless methods).
    param: Option<String>,
    run_on_main_thread: bool,
    allow_pre_server_init: bool,
    handler: Handler,
}

/// The method names and parameter names exactly as `rpc.discover` reports them.
static SCHEMA: LazyLock<JsonElement> = LazyLock::new(|| {
    gson::parse(include_str!(env!("JSON_RPC_API_SCHEMA")), false)
        .expect("embedded schema parses")
        .expect("embedded schema is not empty")
});

/// The `rpc.discover` result: `openrpc`, `info`, `methods`, `components` (vanilla's record order). Only the
/// incoming methods and outgoing notifications flagged discoverable are listed, as in the generated report.
pub fn discover_result() -> JsonElement {
    let JsonElement::Object(schema) = &*SCHEMA else {
        unreachable!("schema is an object")
    };
    let mut out = JsonObject::new();
    for key in ["openrpc", "info", "methods", "components"] {
        if let Some(v) = schema.get(key) {
            out.add(key, v.clone());
        }
    }
    JsonElement::Object(out)
}

fn schema_param_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Some(JsonElement::Array(methods)) = SCHEMA.as_object().and_then(|o| o.get("methods"))
    else {
        return names;
    };
    for m in methods {
        let Some(m) = m.as_object() else { continue };
        let (Some(JsonElement::String(name)), Some(JsonElement::Array(params))) =
            (m.get("name"), m.get("params"))
        else {
            continue;
        };
        if let Some(JsonElement::String(pname)) = params
            .first()
            .and_then(|p| p.as_object())
            .and_then(|p| p.get("name"))
        {
            names.insert(
                name.strip_prefix("minecraft:").unwrap_or(name).to_owned(),
                pname.clone(),
            );
        }
    }
    names
}

static METHODS: LazyLock<HashMap<String, Method>> = LazyLock::new(build_methods);

fn build_methods() -> HashMap<String, Method> {
    let params = schema_param_names();
    let mut map: HashMap<String, Method> = HashMap::new();
    let mut add = |path: &str, handler: Handler| {
        let param = params.get(path).cloned();
        map.insert(
            path.to_owned(),
            Method {
                param,
                run_on_main_thread: true,
                allow_pre_server_init: false,
                handler,
            },
        );
    };
    macro_rules! noarg {
        ($path:expr, $f:expr) => {
            add($path, Box::new(|c, _| Ok($f(c))))
        };
    }
    macro_rules! withparam {
        ($path:expr, $f:expr) => {
            add($path, Box::new(|c, p| $f(c, p)))
        };
    }

    noarg!("allowlist", allowlist_get);
    withparam!("allowlist/set", allowlist_set);
    withparam!("allowlist/add", |c: &Ctx, p: &JsonElement| {
        for dto in decode_list_of(p, PlayerDto::decode)? {
            if let Some(user) = c.get_user(&dto) {
                allowlist_add_user(c, user);
            }
        }
        Ok(allowlist_get(c))
    });
    withparam!("allowlist/remove", |c: &Ctx, p: &JsonElement| {
        for dto in decode_list_of(p, PlayerDto::decode)? {
            if let Some(user) = c.get_user(&dto) {
                allowlist_remove_user(c, &user);
            }
        }
        kick_unlisted(c);
        Ok(allowlist_get(c))
    });
    noarg!("allowlist/clear", |c: &Ctx| {
        c.log("Clear allowlist");
        c.api.allowlist_clear();
        allowlist_get(c)
    });

    noarg!("bans", bans_get);
    withparam!("bans/set", bans_set);
    withparam!("bans/add", bans_add);
    withparam!("bans/remove", bans_remove);
    noarg!("bans/clear", |c: &Ctx| {
        c.api.clear_user_bans();
        bans_get(c)
    });

    noarg!("ip_bans", ip_bans_get);
    withparam!("ip_bans/set", ip_bans_set);
    withparam!("ip_bans/add", ip_bans_add);
    withparam!("ip_bans/remove", ip_bans_remove);
    noarg!("ip_bans/clear", |c: &Ctx| {
        c.log("Clear ip ban list");
        c.api.clear_ip_bans();
        ip_bans_get(c)
    });

    noarg!("players", |c: &Ctx| list(
        c.players_dto().iter().map(PlayerDto::encode).collect()
    ));
    withparam!("players/kick", players_kick);

    noarg!("operators", operators_get);
    withparam!("operators/set", operators_set);
    withparam!("operators/add", operators_add);
    withparam!("operators/remove", operators_remove);
    noarg!("operators/clear", |c: &Ctx| {
        c.log("Clear operator list");
        c.api.clear_operators();
        operators_get(c)
    });

    noarg!("server/status", server_state);
    withparam!("server/save", |c: &Ctx, p: &JsonElement| {
        let flush = decode_bool(p)?;
        c.log(format!(
            "Save everything. SuppressLogs: true, flush: {flush}, force: true"
        ));
        Ok(bool_e(c.api.save_everything(true, flush, true)))
    });
    noarg!("server/stop", |c: &Ctx| {
        // vanilla submits the halt to the main thread and answers immediately; we are already on it
        c.log("Halt server. WaitForShutdown: false");
        c.api.halt(false);
        bool_e(true)
    });
    withparam!("server/system_message", system_message);

    for &(path, key, kind, template, kick) in SETTINGS {
        let get_path = format!("serversettings/{path}");
        add(
            &get_path,
            Box::new(move |c, _| Ok(encode_setting(&c.api.get_setting(key)))),
        );
        let set_path = format!("serversettings/{path}/set");
        add(
            &set_path,
            Box::new(move |c, p| {
                let value = decode_setting(kind, p)?;
                Ok(set_setting(c, key, template, kick, value))
            }),
        );
    }

    noarg!("gamerules", gamerules_get);
    withparam!("gamerules/update", gamerules_update);

    // `rpc.discover`: undiscoverable, allowed before the server exists, not run on the main thread
    map.insert(
        "rpc.discover".to_owned(),
        Method {
            param: None,
            run_on_main_thread: false,
            allow_pre_server_init: true,
            handler: Box::new(|_, _| Ok(discover_result())),
        },
    );
    map
}

/// Names of every incoming method (without the `minecraft:` namespace), for tests and diagnostics.
pub fn incoming_method_names() -> Vec<&'static str> {
    METHODS.keys().map(String::as_str).collect()
}

// ---- request handling (Connection) ----------------------------------------------------------------------------------------

/// What to do after handling one websocket text message.
#[derive(Debug, PartialEq)]
pub enum Outbound {
    /// Nothing to send.
    None,
    /// A text frame to send back.
    Reply(String),
    /// Vanilla closes the channel (an exception escaped the handler).
    Close,
}

/// The protocol logic of one connection: parses messages and answers them with the [`ManagementApi`].
pub struct Dispatcher {
    api: Arc<dyn ManagementApi>,
}

fn is_valid_request_id(id: &JsonElement) -> bool {
    matches!(
        id,
        JsonElement::Null | JsonElement::Number(_) | JsonElement::String(_)
    )
}

impl Dispatcher {
    pub fn new(api: Arc<dyn ManagementApi>) -> Dispatcher {
        Dispatcher { api }
    }

    /// `WebSocketToJsonCodec` + `Connection.channelRead0` for one text frame.
    pub fn handle_text(&self, text: &str, client: ClientInfo) -> Outbound {
        let parsed = match gson::parse(text, true) {
            Ok(Some(v)) => v,
            // an empty document decodes to JsonNull (`JsonParser.parseString`)
            Ok(None) => JsonElement::Null,
            Err(e) => {
                return Outbound::Reply(
                    JsonRpcError::ParseError
                        .create(
                            &JsonElement::Null,
                            Some(&format!("com.google.gson.JsonSyntaxException: {e}")),
                        )
                        .to_compact_string(),
                );
            }
        };
        match parsed {
            JsonElement::Object(o) => match self.handle_json_object(&o, client) {
                Some(response) => Outbound::Reply(response.to_compact_string()),
                None => Outbound::None,
            },
            JsonElement::Array(entries) => {
                let mut responses = Vec::new();
                for entry in &entries {
                    let JsonElement::Object(o) = entry else {
                        return Outbound::Close;
                    };
                    if let Some(r) = self.handle_json_object(o, client) {
                        responses.push(r);
                    }
                }
                Outbound::Reply(JsonElement::Array(responses).to_compact_string())
            }
            _ => Outbound::Reply(
                JsonRpcError::InvalidRequest
                    .create(&JsonElement::Null, None)
                    .to_compact_string(),
            ),
        }
    }

    /// `Connection.handleJsonObject`.
    pub fn handle_json_object(
        &self,
        object: &JsonObject,
        client: ClientInfo,
    ) -> Option<JsonElement> {
        let internal = || {
            Some(JsonRpcError::InternalError.create(
                &JsonElement::Null,
                Some("Unknown error handling request - check server logs for stack trace"),
            ))
        };
        let id = object.get("id");
        let method = match object.get("method") {
            None => None,
            Some(JsonElement::String(s)) | Some(JsonElement::Number(s)) => Some(s.clone()),
            Some(JsonElement::Bool(b)) => Some(b.to_string()),
            Some(other) => {
                log::error!(
                    "Error while handling rpc request\ncom.google.gson.JsonSyntaxException: Expected method to be a string, was {}",
                    other.to_compact_string()
                );
                return internal();
            }
        };
        let result = object.get("result");
        let params = object.get("params");
        let error = match object.get("error") {
            None => None,
            Some(JsonElement::Object(o)) => Some(o),
            Some(other) => {
                log::error!(
                    "Error while handling rpc request\ncom.google.gson.JsonSyntaxException: Expected error to be a JsonObject, was {}",
                    other.to_compact_string()
                );
                return internal();
            }
        };
        if let (Some(method), None, None) = (&method, result, error) {
            return match id {
                Some(id) if !is_valid_request_id(id) => Some(JsonRpcError::InvalidRequest.create(
                    &JsonElement::Null,
                    Some("Invalid request id - only String, Number and NULL supported"),
                )),
                _ => self.handle_incoming_request(id, method, params, client),
            };
        }
        if method.is_none()
            && error.is_none()
            && let (Some(result), Some(id)) = (result, id)
        {
            if matches!(id, JsonElement::Number(_)) {
                // we never send requests that expect a reply, so no response can match one
                log::warn!(
                    "Received unknown response (id: {}): {}",
                    id.to_compact_string(),
                    result.to_compact_string()
                );
            } else {
                log::warn!(
                    "Received respose {} with id {} we did not request",
                    result.to_compact_string(),
                    id.to_compact_string()
                );
            }
            return None;
        }
        if method.is_none()
            && result.is_none()
            && let Some(error) = error
        {
            // `handleError`
            log::error!(
                "Received error (id: {}): {}",
                id.map_or("null".to_owned(), JsonElement::to_compact_string),
                JsonElement::Object(error.clone()).to_compact_string()
            );
            return None;
        }
        Some(JsonRpcError::InvalidRequest.create(id.unwrap_or(&JsonElement::Null), None))
    }

    /// `Connection.handleIncomingRequest`.
    fn handle_incoming_request(
        &self,
        id: Option<&JsonElement>,
        method: &str,
        params: Option<&JsonElement>,
        client: ClientInfo,
    ) -> Option<JsonElement> {
        let send_response = id.is_some();
        let outcome = self.dispatch_incoming_request(method, params, client);
        let id_value = id.cloned().unwrap_or(JsonElement::Null);
        match outcome {
            Ok(result) => send_response.then(|| {
                let mut response = JsonObject::new();
                response.add_property_str("jsonrpc", Some("2.0"));
                response.add("id", id_value);
                response.add("result", result);
                JsonElement::Object(response)
            }),
            Err(RpcError::InvalidParameter(m)) => {
                log::debug!("Invalid parameter invocation {method}: {m}");
                send_response.then(|| JsonRpcError::InvalidParams.create(&id_value, Some(&m)))
            }
            Err(RpcError::Encode(m)) => {
                log::error!("Failed to encode json rpc response {method}\n{m}");
                send_response.then(|| JsonRpcError::InternalError.create(&id_value, Some(&m)))
            }
            Err(RpcError::InvalidRequest(m)) => {
                send_response.then(|| JsonRpcError::InvalidRequest.create(&id_value, Some(&m)))
            }
            Err(RpcError::MethodNotFound(m)) => {
                send_response.then(|| JsonRpcError::MethodNotFound.create(&id_value, Some(&m)))
            }
            Err(RpcError::Internal(m)) => {
                log::error!("Error while dispatching rpc method {method}\n{m}");
                send_response.then(|| JsonRpcError::InternalError.create(&id_value, None))
            }
        }
    }

    /// `Connection.dispatchIncomingRequest` + `IncomingRpcMethod.apply`.
    pub fn dispatch_incoming_request(
        &self,
        method: &str,
        params: Option<&JsonElement>,
        client: ClientInfo,
    ) -> RpcResult {
        let Some(identifier) = Identifier::try_parse(method) else {
            return Err(RpcError::InvalidRequest(format!(
                "Failed to parse method value: {method}"
            )));
        };
        let entry = (identifier.namespace() == "minecraft")
            .then(|| METHODS.get(identifier.path()))
            .flatten();
        let Some(entry) = entry else {
            return Err(RpcError::MethodNotFound(format!(
                "Method not found: {method}"
            )));
        };
        if !self.api.server_attached() && !entry.allow_pre_server_init {
            return Err(RpcError::InvalidRequest(format!(
                "Method cannot be dispatched pre server initialization: {method}"
            )));
        }
        let param = extract_param(entry, params)?;
        let ctx = Ctx {
            api: self.api.clone(),
            client,
        };
        if entry.run_on_main_thread {
            let mut slot: Option<RpcResult> = None;
            {
                let slot_ref = &mut slot;
                let (ctx_ref, param_ref, handler) = (&ctx, &param, &entry.handler);
                self.api.run_on_server_thread(Box::new(move || {
                    *slot_ref = Some(handler(ctx_ref, param_ref))
                }));
            }
            slot.unwrap_or_else(|| {
                Err(RpcError::Internal(
                    "the server thread dropped the task".to_owned(),
                ))
            })
        } else {
            (entry.handler)(&ctx, &param)
        }
    }
}

/// The parameter handling of `Method.apply` / `ParameterlessMethod.apply`.
fn extract_param(method: &Method, params: Option<&JsonElement>) -> Result<JsonElement, RpcError> {
    let invalid = |m: &str| RpcError::InvalidParameter(m.to_owned());
    match &method.param {
        None => match params {
            None => Ok(JsonElement::Null),
            Some(JsonElement::Array(a)) if a.is_empty() => Ok(JsonElement::Null),
            Some(_) => Err(invalid("Expected no params, or an empty array")),
        },
        Some(name) => match params {
            Some(JsonElement::Object(o)) => o.get(name).cloned().ok_or_else(|| {
                RpcError::InvalidParameter(format!(
                    "Params passed by-name, but expected param [{name}] does not exist"
                ))
            }),
            Some(JsonElement::Array(a)) => match a.as_slice() {
                [one] => Ok(one.clone()),
                _ => Err(invalid("Expected exactly one element in the params array")),
            },
            _ => Err(invalid("Expected params as array or named")),
        },
    }
}

/// The player ids of a list (helper for notification payloads).
pub fn player_uuid(p: &ManagementPlayer) -> Uuid {
    p.profile.id
}
