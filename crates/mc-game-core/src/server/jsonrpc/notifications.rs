//! `JsonRpcNotificationService` and `OutgoingRpcMethods`: the server -> client notifications.

use std::sync::Arc;

use mc_core::math::java::java_float_to_string;

use super::api::{GameRuleInfo, GameRuleType, ManagementApi};
use super::dto::*;
use super::server::Connections;
use crate::server::notifications::{GameRuleValue, NotificationService};
use crate::server::players::gson::{JsonElement, JsonObject};
use crate::server::players::{IpBanListEntry, NameAndId, ServerOpListEntry, UserBanListEntry};

/// `JsonRpcUtils.createRequest(null, method, params)`: a notification (no `id`, no `params` when empty).
pub fn create_notification(method: &str, params: Option<JsonElement>) -> String {
    let mut request = JsonObject::new();
    request.add_property_str("jsonrpc", Some("2.0"));
    request.add_property_str("method", Some(method));
    if let Some(p) = params {
        request.add("params", JsonElement::Array(vec![p]));
    }
    JsonElement::Object(request).to_compact_string()
}

/// `OutgoingRpcMethods`: the `outgoing_rpc_methods` registry, in vanilla order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutgoingRpcMethod {
    ServerStarted,
    ServerStopping,
    ServerSaving,
    ServerSaved,
    ServerActivity,
    WorldUpgradeStarted,
    WorldUpgradeProgress,
    WorldUpgradeFinished,
    WorldUpgradeFailed,
    PlayersJoined,
    PlayersLeft,
    OperatorsAdded,
    OperatorsRemoved,
    AllowlistAdded,
    AllowlistRemoved,
    IpBansAdded,
    IpBansRemoved,
    BansAdded,
    BansRemoved,
    GamerulesUpdated,
    ServerStatus,
}

impl OutgoingRpcMethod {
    pub const ALL: [OutgoingRpcMethod; 21] = [
        OutgoingRpcMethod::ServerStarted,
        OutgoingRpcMethod::ServerStopping,
        OutgoingRpcMethod::ServerSaving,
        OutgoingRpcMethod::ServerSaved,
        OutgoingRpcMethod::ServerActivity,
        OutgoingRpcMethod::WorldUpgradeStarted,
        OutgoingRpcMethod::WorldUpgradeProgress,
        OutgoingRpcMethod::WorldUpgradeFinished,
        OutgoingRpcMethod::WorldUpgradeFailed,
        OutgoingRpcMethod::PlayersJoined,
        OutgoingRpcMethod::PlayersLeft,
        OutgoingRpcMethod::OperatorsAdded,
        OutgoingRpcMethod::OperatorsRemoved,
        OutgoingRpcMethod::AllowlistAdded,
        OutgoingRpcMethod::AllowlistRemoved,
        OutgoingRpcMethod::IpBansAdded,
        OutgoingRpcMethod::IpBansRemoved,
        OutgoingRpcMethod::BansAdded,
        OutgoingRpcMethod::BansRemoved,
        OutgoingRpcMethod::GamerulesUpdated,
        OutgoingRpcMethod::ServerStatus,
    ];

    /// The registry key, which is also the JSON-RPC `method` on the wire.
    pub const fn name(self) -> &'static str {
        match self {
            OutgoingRpcMethod::ServerStarted => "minecraft:notification/server/started",
            OutgoingRpcMethod::ServerStopping => "minecraft:notification/server/stopping",
            OutgoingRpcMethod::ServerSaving => "minecraft:notification/server/saving",
            OutgoingRpcMethod::ServerSaved => "minecraft:notification/server/saved",
            OutgoingRpcMethod::ServerActivity => "minecraft:notification/server/activity",
            OutgoingRpcMethod::WorldUpgradeStarted => {
                "minecraft:notification/world/upgrade_started"
            }
            OutgoingRpcMethod::WorldUpgradeProgress => {
                "minecraft:notification/world/upgrade_progress"
            }
            OutgoingRpcMethod::WorldUpgradeFinished => {
                "minecraft:notification/world/upgrade_finished"
            }
            OutgoingRpcMethod::WorldUpgradeFailed => "minecraft:notification/world/upgrade_failed",
            OutgoingRpcMethod::PlayersJoined => "minecraft:notification/players/joined",
            OutgoingRpcMethod::PlayersLeft => "minecraft:notification/players/left",
            OutgoingRpcMethod::OperatorsAdded => "minecraft:notification/operators/added",
            OutgoingRpcMethod::OperatorsRemoved => "minecraft:notification/operators/removed",
            OutgoingRpcMethod::AllowlistAdded => "minecraft:notification/allowlist/added",
            OutgoingRpcMethod::AllowlistRemoved => "minecraft:notification/allowlist/removed",
            OutgoingRpcMethod::IpBansAdded => "minecraft:notification/ip_bans/added",
            OutgoingRpcMethod::IpBansRemoved => "minecraft:notification/ip_bans/removed",
            OutgoingRpcMethod::BansAdded => "minecraft:notification/bans/added",
            OutgoingRpcMethod::BansRemoved => "minecraft:notification/bans/removed",
            OutgoingRpcMethod::GamerulesUpdated => "minecraft:notification/gamerules/updated",
            OutgoingRpcMethod::ServerStatus => "minecraft:notification/server/status",
        }
    }
}

/// Size of the vanilla `outgoing_rpc_methods` registry as modelled here.
pub fn outgoing_rpc_methods_count() -> usize {
    OutgoingRpcMethod::ALL.len()
}

/// `JsonRpcNotificationService`: broadcasts every event to all open connections.
pub struct JsonRpcNotificationService {
    api: Arc<dyn ManagementApi>,
    connections: Arc<Connections>,
}

impl JsonRpcNotificationService {
    pub fn new(api: Arc<dyn ManagementApi>, connections: Arc<Connections>) -> Self {
        JsonRpcNotificationService { api, connections }
    }

    /// `allow_pre_server_init`: the `OutgoingRpcMethod` attribute; otherwise the notification is dropped while the
    /// server object does not exist yet (vanilla fails the send).
    fn broadcast(
        &self,
        method: OutgoingRpcMethod,
        params: Option<JsonElement>,
        allow_pre_server_init: bool,
    ) {
        if !self.api.server_attached() && !allow_pre_server_init {
            return;
        }
        self.connections
            .broadcast(&create_notification(method.name(), params));
    }
}

fn typed_rule(rule: &str, value: &GameRuleValue) -> JsonElement {
    let (rule_type, value) = match value {
        GameRuleValue::Bool(b) => (GameRuleType::Bool, GameRuleValue::Bool(*b)),
        GameRuleValue::Int(i) => (GameRuleType::Int, GameRuleValue::Int(*i)),
    };
    let id = if rule.contains(':') {
        rule.to_owned()
    } else {
        format!("minecraft:{rule}")
    };
    encode_typed_game_rule(&GameRuleInfo {
        id,
        rule_type,
        value,
    })
}

impl NotificationService for JsonRpcNotificationService {
    fn player_joined(&self, player: &NameAndId) {
        self.broadcast(
            OutgoingRpcMethod::PlayersJoined,
            Some(PlayerDto::from_name_and_id(player).encode()),
            false,
        );
    }

    fn player_left(&self, player: &NameAndId) {
        self.broadcast(
            OutgoingRpcMethod::PlayersLeft,
            Some(PlayerDto::from_name_and_id(player).encode()),
            false,
        );
    }

    fn server_started(&self) {
        self.broadcast(OutgoingRpcMethod::ServerStarted, None, false);
    }

    fn server_shutting_down(&self) {
        self.broadcast(OutgoingRpcMethod::ServerStopping, None, false);
    }

    fn server_save_started(&self) {
        self.broadcast(OutgoingRpcMethod::ServerSaving, None, false);
    }

    fn server_save_completed(&self) {
        self.broadcast(OutgoingRpcMethod::ServerSaved, None, false);
    }

    fn server_activity_occured(&self) {
        self.broadcast(OutgoingRpcMethod::ServerActivity, None, false);
    }

    fn world_upgrade_started(&self) {
        self.broadcast(OutgoingRpcMethod::WorldUpgradeStarted, None, true);
    }

    fn world_upgrade_progress(&self, progress_percentage: f32) {
        self.broadcast(
            OutgoingRpcMethod::WorldUpgradeProgress,
            Some(JsonElement::Number(java_float_to_string(
                progress_percentage,
            ))),
            true,
        );
    }

    fn world_upgrade_finished(&self) {
        self.broadcast(OutgoingRpcMethod::WorldUpgradeFinished, None, true);
    }

    fn world_upgrade_failed(&self, reason: &str) {
        self.broadcast(
            OutgoingRpcMethod::WorldUpgradeFailed,
            Some(JsonElement::String(reason.to_owned().into())),
            true,
        );
    }

    fn player_oped(&self, operator: &ServerOpListEntry) {
        if let Some(dto) = OperatorDto::from_entry(operator) {
            self.broadcast(OutgoingRpcMethod::OperatorsAdded, Some(dto.encode()), false);
        }
    }

    fn player_deoped(&self, operator: &ServerOpListEntry) {
        if let Some(dto) = OperatorDto::from_entry(operator) {
            self.broadcast(
                OutgoingRpcMethod::OperatorsRemoved,
                Some(dto.encode()),
                false,
            );
        }
    }

    fn player_added_to_allowlist(&self, player: &NameAndId) {
        self.broadcast(
            OutgoingRpcMethod::AllowlistAdded,
            Some(PlayerDto::from_name_and_id(player).encode()),
            false,
        );
    }

    fn player_removed_from_allowlist(&self, player: &NameAndId) {
        self.broadcast(
            OutgoingRpcMethod::AllowlistRemoved,
            Some(PlayerDto::from_name_and_id(player).encode()),
            false,
        );
    }

    fn ip_banned(&self, ban: &IpBanListEntry) {
        if let Some(dto) = IpBanDto::from_entry(ban) {
            self.broadcast(OutgoingRpcMethod::IpBansAdded, Some(dto.encode()), false);
        }
    }

    fn ip_unbanned(&self, ip: &str) {
        self.broadcast(
            OutgoingRpcMethod::IpBansRemoved,
            Some(JsonElement::String(ip.to_owned().into())),
            false,
        );
    }

    fn player_banned(&self, ban: &UserBanListEntry) {
        if let Some(dto) = UserBanDto::from_entry(ban) {
            self.broadcast(OutgoingRpcMethod::BansAdded, Some(dto.encode()), false);
        }
    }

    fn player_unbanned(&self, player: &NameAndId) {
        self.broadcast(
            OutgoingRpcMethod::BansRemoved,
            Some(PlayerDto::from_name_and_id(player).encode()),
            false,
        );
    }

    fn on_game_rule_changed(&self, rule: &str, value: &GameRuleValue) {
        self.broadcast(
            OutgoingRpcMethod::GamerulesUpdated,
            Some(typed_rule(rule, value)),
            false,
        );
    }

    fn status_heartbeat(&self) {
        let version = self.api.version();
        let state = if self.api.is_ready() {
            let players: Vec<PlayerDto> = self
                .api
                .players()
                .iter()
                .map(|p| PlayerDto::from_name_and_id(&p.profile))
                .collect();
            encode_server_state(true, &players, &version)
        } else {
            encode_server_state(false, &[], &version)
        };
        self.broadcast(OutgoingRpcMethod::ServerStatus, Some(state), true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_wire_format() {
        assert_eq!(
            create_notification("minecraft:notification/server/started", None),
            "{\"jsonrpc\":\"2.0\",\"method\":\"minecraft:notification/server/started\"}"
        );
        let p = PlayerDto {
            id: None,
            name: Some("a".into()),
        }
        .encode();
        assert_eq!(
            create_notification("minecraft:notification/players/joined", Some(p)),
            "{\"jsonrpc\":\"2.0\",\"method\":\"minecraft:notification/players/joined\",\"params\":[{\"name\":\"a\"}]}"
        );
        assert_eq!(
            typed_rule("advance_time", &GameRuleValue::Bool(true)).to_compact_string(),
            "{\"type\":\"boolean\",\"value\":true,\"key\":\"minecraft:advance_time\"}"
        );
    }

    #[test]
    fn every_outgoing_method_has_its_own_name() {
        let names: std::collections::BTreeSet<_> = OutgoingRpcMethod::ALL
            .into_iter()
            .map(OutgoingRpcMethod::name)
            .collect();
        assert_eq!(names.len(), OutgoingRpcMethod::ALL.len());
    }
}
