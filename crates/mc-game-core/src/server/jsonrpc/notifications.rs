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
    fn broadcast(&self, method: &str, params: Option<JsonElement>, allow_pre_server_init: bool) {
        if !self.api.server_attached() && !allow_pre_server_init {
            return;
        }
        self.connections.broadcast(&create_notification(method, params));
    }
}

fn typed_rule(rule: &str, value: &GameRuleValue) -> JsonElement {
    let (rule_type, value) = match value {
        GameRuleValue::Bool(b) => (GameRuleType::Bool, GameRuleValue::Bool(*b)),
        GameRuleValue::Int(i) => (GameRuleType::Int, GameRuleValue::Int(*i)),
    };
    let id = if rule.contains(':') { rule.to_owned() } else { format!("minecraft:{rule}") };
    encode_typed_game_rule(&GameRuleInfo { id, rule_type, value })
}

impl NotificationService for JsonRpcNotificationService {
    fn player_joined(&self, player: &NameAndId) {
        self.broadcast("minecraft:notification/players/joined", Some(PlayerDto::from_name_and_id(player).encode()), false);
    }

    fn player_left(&self, player: &NameAndId) {
        self.broadcast("minecraft:notification/players/left", Some(PlayerDto::from_name_and_id(player).encode()), false);
    }

    fn server_started(&self) {
        self.broadcast("minecraft:notification/server/started", None, false);
    }

    fn server_shutting_down(&self) {
        self.broadcast("minecraft:notification/server/stopping", None, false);
    }

    fn server_save_started(&self) {
        self.broadcast("minecraft:notification/server/saving", None, false);
    }

    fn server_save_completed(&self) {
        self.broadcast("minecraft:notification/server/saved", None, false);
    }

    fn server_activity_occured(&self) {
        self.broadcast("minecraft:notification/server/activity", None, false);
    }

    fn world_upgrade_started(&self) {
        self.broadcast("minecraft:notification/world/upgrade_started", None, true);
    }

    fn world_upgrade_progress(&self, progress_percentage: f32) {
        self.broadcast("minecraft:notification/world/upgrade_progress", Some(JsonElement::Number(java_float_to_string(progress_percentage))), true);
    }

    fn world_upgrade_finished(&self) {
        self.broadcast("minecraft:notification/world/upgrade_finished", None, true);
    }

    fn world_upgrade_failed(&self, reason: &str) {
        self.broadcast("minecraft:notification/world/upgrade_failed", Some(JsonElement::String(reason.to_owned())), true);
    }

    fn player_oped(&self, operator: &ServerOpListEntry) {
        if let Some(dto) = OperatorDto::from_entry(operator) {
            self.broadcast("minecraft:notification/operators/added", Some(dto.encode()), false);
        }
    }

    fn player_deoped(&self, operator: &ServerOpListEntry) {
        if let Some(dto) = OperatorDto::from_entry(operator) {
            self.broadcast("minecraft:notification/operators/removed", Some(dto.encode()), false);
        }
    }

    fn player_added_to_allowlist(&self, player: &NameAndId) {
        self.broadcast("minecraft:notification/allowlist/added", Some(PlayerDto::from_name_and_id(player).encode()), false);
    }

    fn player_removed_from_allowlist(&self, player: &NameAndId) {
        self.broadcast("minecraft:notification/allowlist/removed", Some(PlayerDto::from_name_and_id(player).encode()), false);
    }

    fn ip_banned(&self, ban: &IpBanListEntry) {
        if let Some(dto) = IpBanDto::from_entry(ban) {
            self.broadcast("minecraft:notification/ip_bans/added", Some(dto.encode()), false);
        }
    }

    fn ip_unbanned(&self, ip: &str) {
        self.broadcast("minecraft:notification/ip_bans/removed", Some(JsonElement::String(ip.to_owned())), false);
    }

    fn player_banned(&self, ban: &UserBanListEntry) {
        if let Some(dto) = UserBanDto::from_entry(ban) {
            self.broadcast("minecraft:notification/bans/added", Some(dto.encode()), false);
        }
    }

    fn player_unbanned(&self, player: &NameAndId) {
        self.broadcast("minecraft:notification/bans/removed", Some(PlayerDto::from_name_and_id(player).encode()), false);
    }

    fn on_game_rule_changed(&self, rule: &str, value: &GameRuleValue) {
        self.broadcast("minecraft:notification/gamerules/updated", Some(typed_rule(rule, value)), false);
    }

    fn status_heartbeat(&self) {
        let version = self.api.version();
        let state = if self.api.is_ready() {
            let players: Vec<PlayerDto> = self.api.players().iter().map(|p| PlayerDto::from_name_and_id(&p.profile)).collect();
            encode_server_state(true, &players, &version)
        } else {
            encode_server_state(false, &[], &version)
        };
        self.broadcast("minecraft:notification/server/status", Some(state), true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_wire_format() {
        assert_eq!(create_notification("minecraft:notification/server/started", None), "{\"jsonrpc\":\"2.0\",\"method\":\"minecraft:notification/server/started\"}");
        let p = PlayerDto { id: None, name: Some("a".into()) }.encode();
        assert_eq!(
            create_notification("minecraft:notification/players/joined", Some(p)),
            "{\"jsonrpc\":\"2.0\",\"method\":\"minecraft:notification/players/joined\",\"params\":[{\"name\":\"a\"}]}"
        );
        assert_eq!(typed_rule("advance_time", &GameRuleValue::Bool(true)).to_compact_string(), "{\"key\":\"minecraft:advance_time\",\"type\":\"boolean\",\"value\":true}");
    }
}
