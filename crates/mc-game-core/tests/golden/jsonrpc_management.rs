//! JSON-RPC management protocol: request handling against an in-memory `ManagementApi`, and a real websocket
//! round trip (authentication, request/response, notification broadcast).

use std::sync::Arc;

use mc_game::server::jsonrpc::ManagementServer;
use mc_game::server::jsonrpc::api::*;
use mc_game::server::jsonrpc::methods::{Dispatcher, Outbound, incoming_method_names};
use mc_game::server::jsonrpc::notifications::JsonRpcNotificationService;
use mc_game::server::jsonrpc::security::AuthenticationHandler;
use mc_game::server::notifications::NotificationService;
use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel};
use mc_game::server::players::gson::{self, JsonElement};
use mc_game::server::players::{
    IpBanListEntry, NameAndId, ServerOpListEntry, StoredUserEntry, UserBanListEntry,
    UserWhiteListEntry,
};
use mc_text::Component;
use mc_world::level_storage::{Difficulty, GameType};
use parking_lot::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct State {
    allow: Vec<NameAndId>,
    bans: Vec<UserBanListEntry>,
    ip_bans: Vec<IpBanListEntry>,
    ops: Vec<ServerOpListEntry>,
    online: Vec<ManagementPlayer>,
    disconnected: Vec<(Uuid, String)>,
    motd: String,
    difficulty: Option<Difficulty>,
    saved: u32,
}

struct Mock {
    state: Mutex<State>,
}

impl Mock {
    fn new() -> Arc<Mock> {
        let mut st = State {
            motd: "A Minecraft Server".into(),
            difficulty: Some(Difficulty::Easy),
            ..State::default()
        };
        st.online.push(ManagementPlayer {
            profile: NameAndId::create_offline("Steve"),
            ip_address: "10.1.1.1".into(),
        });
        Arc::new(Mock {
            state: Mutex::new(st),
        })
    }
}

impl ManagementApi for Mock {
    fn server_attached(&self) -> bool {
        true
    }
    fn is_ready(&self) -> bool {
        true
    }
    fn version(&self) -> ServerVersion {
        ServerVersion {
            name: "26.3".into(),
            protocol: 777,
        }
    }
    fn run_on_server_thread(&self, task: Box<dyn FnOnce() + Send + '_>) {
        task()
    }
    fn allowlist_entries(&self) -> Vec<UserWhiteListEntry> {
        self.state
            .lock()
            .allow
            .iter()
            .cloned()
            .map(UserWhiteListEntry::new)
            .collect()
    }
    fn allowlist_add(&self, entry: UserWhiteListEntry) -> bool {
        self.state.lock().allow.push(entry.user().unwrap().clone());
        true
    }
    fn allowlist_remove(&self, user: &NameAndId) {
        self.state.lock().allow.retain(|u| u != user);
    }
    fn allowlist_clear(&self) {
        self.state.lock().allow.clear();
    }
    fn kick_unlisted_players(&self) {}
    fn user_ban_entries(&self) -> Vec<UserBanListEntry> {
        self.state.lock().bans.clone()
    }
    fn add_user_ban(&self, ban: UserBanListEntry) {
        self.state.lock().bans.push(ban);
    }
    fn remove_user_ban(&self, user: &NameAndId) {
        self.state.lock().bans.retain(|b| b.user() != Some(user));
    }
    fn clear_user_bans(&self) {
        self.state.lock().bans.clear();
    }
    fn ip_ban_entries(&self) -> Vec<IpBanListEntry> {
        self.state.lock().ip_bans.clone()
    }
    fn add_ip_ban(&self, ban: IpBanListEntry) {
        self.state.lock().ip_bans.push(ban);
    }
    fn remove_ip_ban(&self, ip: &str) {
        self.state
            .lock()
            .ip_bans
            .retain(|b| b.user().map(String::as_str) != Some(ip));
    }
    fn clear_ip_bans(&self) {
        self.state.lock().ip_bans.clear();
    }
    fn operator_entries(&self) -> Vec<ServerOpListEntry> {
        self.state.lock().ops.clone()
    }
    fn op(&self, user: NameAndId, level: Option<PermissionLevel>, bypass: Option<bool>) {
        let level = level.map_or(
            LevelBasedPermissionSet::OWNER,
            LevelBasedPermissionSet::for_level,
        );
        self.state
            .lock()
            .ops
            .push(ServerOpListEntry::new(user, level, bypass.unwrap_or(false)));
    }
    fn deop(&self, user: &NameAndId) {
        self.state.lock().ops.retain(|o| o.user() != Some(user));
    }
    fn clear_operators(&self) {
        self.state.lock().ops.clear();
    }
    fn players(&self) -> Vec<ManagementPlayer> {
        self.state.lock().online.clone()
    }
    fn player_by_id(&self, id: Uuid) -> Option<ManagementPlayer> {
        self.state
            .lock()
            .online
            .iter()
            .find(|p| p.profile.id == id)
            .cloned()
    }
    fn player_by_name(&self, name: &str) -> Option<ManagementPlayer> {
        self.state
            .lock()
            .online
            .iter()
            .find(|p| p.profile.name == name)
            .cloned()
    }
    fn players_with_address(&self, ip: &str) -> Vec<ManagementPlayer> {
        self.state
            .lock()
            .online
            .iter()
            .filter(|p| p.ip_address == ip)
            .cloned()
            .collect()
    }
    fn cached_user_by_id(&self, _id: Uuid) -> Option<NameAndId> {
        None
    }
    fn fetch_user_by_id(&self, _id: Uuid) -> Option<NameAndId> {
        None
    }
    fn fetch_user_by_name(&self, name: &str) -> Option<NameAndId> {
        Some(NameAndId::create_offline(name))
    }
    fn remove_player(&self, id: Uuid) {
        self.state.lock().online.retain(|p| p.profile.id != id);
    }
    fn disconnect_player(&self, id: Uuid, reason: Component) {
        self.state
            .lock()
            .disconnected
            .push((id, reason.get_string()));
    }
    fn send_system_message(&self, _players: &[Uuid], _message: Component, _overlay: bool) {}
    fn broadcast_system_message(&self, _message: Component, _overlay: bool) {}
    fn save_everything(&self, _s: bool, _f: bool, _force: bool) -> bool {
        self.state.lock().saved += 1;
        true
    }
    fn halt(&self, _wait: bool) {}
    fn game_rules(&self) -> Vec<GameRuleInfo> {
        vec![GameRuleInfo {
            id: "minecraft:advance_time".into(),
            rule_type: GameRuleType::Bool,
            value: GameRuleValue::Bool(true),
        }]
    }
    fn update_game_rule(&self, key: &str, value: GameRuleValue) -> Result<GameRuleInfo, String> {
        match key {
            "minecraft:advance_time" | "advance_time"
                if matches!(value, GameRuleValue::Bool(_)) =>
            {
                Ok(GameRuleInfo {
                    id: "minecraft:advance_time".into(),
                    rule_type: GameRuleType::Bool,
                    value,
                })
            }
            _ => Err(format!("Unknown or mistyped rule {key}")),
        }
    }
    fn get_setting(&self, key: SettingKey) -> SettingValue {
        let st = self.state.lock();
        match key {
            SettingKey::Motd => SettingValue::Str(st.motd.clone()),
            SettingKey::Difficulty => SettingValue::Difficulty(st.difficulty.unwrap()),
            SettingKey::GameMode => SettingValue::GameType(GameType::Survival),
            SettingKey::OperatorUserPermissions => {
                SettingValue::PermissionLevel(PermissionLevel::Owners)
            }
            SettingKey::MaxPlayers => SettingValue::Int(20),
            _ => SettingValue::Bool(false),
        }
    }
    fn set_setting(&self, key: SettingKey, value: SettingValue) {
        let mut st = self.state.lock();
        match (key, value) {
            (SettingKey::Motd, SettingValue::Str(s)) => st.motd = s,
            (SettingKey::Difficulty, SettingValue::Difficulty(d)) => st.difficulty = Some(d),
            _ => {}
        }
    }
    fn set_status_heartbeat_interval(&self, _seconds: i32) -> bool {
        true
    }
}

fn call(d: &Dispatcher, text: &str) -> String {
    match d.handle_text(text, ClientInfo { connection_id: 1 }) {
        Outbound::Reply(r) => r,
        other => format!("{other:?}"),
    }
}

fn dispatcher() -> (Arc<Mock>, Dispatcher) {
    let mock = Mock::new();
    (mock.clone(), Dispatcher::new(mock))
}

/// The server -> client notifications the schema lists next to the incoming methods.
const NOTIFICATIONS: &[&str] = &[
    "notification/server/started",
    "notification/server/stopping",
    "notification/server/saving",
    "notification/server/saved",
    "notification/server/activity",
    "notification/world/upgrade_started",
    "notification/world/upgrade_progress",
    "notification/world/upgrade_finished",
    "notification/world/upgrade_failed",
    "notification/players/joined",
    "notification/players/left",
    "notification/operators/added",
    "notification/operators/removed",
    "notification/allowlist/added",
    "notification/allowlist/removed",
    "notification/ip_bans/added",
    "notification/ip_bans/removed",
    "notification/bans/added",
    "notification/bans/removed",
    "notification/gamerules/updated",
    "notification/server/status",
];

#[test]
fn every_schema_method_is_implemented() {
    let schema = gson::parse(include_str!(env!("JSON_RPC_API_SCHEMA")), false)
        .unwrap()
        .unwrap();
    let methods = schema.as_object().unwrap().get("methods").unwrap();
    let JsonElement::Array(methods) = methods else {
        panic!()
    };
    let incoming = incoming_method_names();
    let mut implemented = 0;
    for m in methods {
        let name = m
            .as_object()
            .unwrap()
            .get("name")
            .unwrap()
            .as_string()
            .unwrap();
        let path = name.strip_prefix("minecraft:").unwrap();
        if incoming.contains(&path) {
            implemented += 1;
        } else {
            assert!(
                NOTIFICATIONS.contains(&path),
                "{path} is neither implemented nor a known notification"
            );
        }
    }
    assert_eq!(implemented, 68);
    assert_eq!(methods.len(), 68 + NOTIFICATIONS.len(), "method count");
}

#[test]
fn discover_and_errors() {
    let (_, d) = dispatcher();
    let r = call(&d, r#"{"jsonrpc":"2.0","id":1,"method":"rpc.discover"}"#);
    assert!(r.starts_with(r#"{"jsonrpc":"2.0","id":1,"result":{"openrpc":"1.3.2","info":{"title":"Minecraft Server JSON-RPC","version":"3.1.0"},"methods":["#), "{r}");
    assert_eq!(
        call(&d, r#"{"jsonrpc":"2.0","id":2,"method":"nope"}"#),
        r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found","data":"Method not found: nope"}}"#
    );
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":3,"method":"players","params":[1]}"#
        ),
        r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"Invalid params","data":"Expected no params, or an empty array"}}"#
    );
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":4,"method":"allowlist/add","params":{"wrong":[]}}"#
        ),
        r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"Invalid params","data":"Params passed by-name, but expected param [add] does not exist"}}"#
    );
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":5,"method":"allowlist/add","params":[[],[]]}"#
        ),
        r#"{"jsonrpc":"2.0","id":5,"error":{"code":-32602,"message":"Invalid params","data":"Expected exactly one element in the params array"}}"#
    );
    assert_eq!(
        call(&d, r#"{"jsonrpc":"2.0","id":6,"method":"allowlist/add"}"#),
        r#"{"jsonrpc":"2.0","id":6,"error":{"code":-32602,"message":"Invalid params","data":"Expected params as array or named"}}"#
    );
    assert!(call(&d, "{").contains(r#""code":-32700"#));
    assert_eq!(
        call(&d, "5"),
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}}"#
    );
    assert_eq!(
        call(&d, r#"{"jsonrpc":"2.0","id":true,"method":"players"}"#),
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request","data":"Invalid request id - only String, Number and NULL supported"}}"#
    );
    // a notification (no id) is executed but never answered
    assert_eq!(
        d.handle_text(
            r#"{"jsonrpc":"2.0","method":"players"}"#,
            ClientInfo { connection_id: 1 }
        ),
        Outbound::None
    );
    // namespaced names work, a foreign namespace does not
    assert!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":7,"method":"minecraft:players"}"#
        )
        .contains("\"result\":[")
    );
    assert!(call(&d, r#"{"jsonrpc":"2.0","id":8,"method":"other:players"}"#).contains("-32601"));
}

#[test]
fn allowlist_ops_bans_players_and_settings() {
    let (mock, d) = dispatcher();
    let steve = NameAndId::create_offline("Steve");
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":1,"method":"allowlist/add","params":[[{"name":"Steve"},{"name":"Alex"}]]}"#,
    );
    assert!(
        r.contains(&format!(r#"{{"id":"{}","name":"Steve"}}"#, steve.id)),
        "{r}"
    );
    assert_eq!(mock.state.lock().allow.len(), 2);
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":2,"method":"allowlist/set","params":{"players":[{"name":"Alex"}]}}"#,
    );
    assert!(!r.contains("Steve") && r.contains("Alex"), "{r}");

    // ops: set drops the stale entry, adds the new one; level 9 clamps to 4
    call(
        &d,
        r#"{"jsonrpc":"2.0","id":3,"method":"operators/add","params":[[{"player":{"name":"Steve"},"permissionLevel":2}]]}"#,
    );
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":4,"method":"operators/set","params":[[{"player":{"name":"Alex"},"permissionLevel":9,"bypassesPlayerLimit":true}]]}"#,
    );
    assert!(
        r.contains(r#""permissionLevel":4,"bypassesPlayerLimit":true"#) && !r.contains("Steve"),
        "{r}"
    );

    // banning an online player kicks them with the vanilla message
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":5,"method":"bans/add","params":[[{"player":{"name":"Steve"},"reason":"x","expires":"2999-01-01T00:00:00Z"}]]}"#,
    );
    assert!(
        r.contains(r#""reason":"x","source":"Management server","expires":"2999-01-01T00:00:00Z""#),
        "{r}"
    );
    assert_eq!(
        mock.state.lock().disconnected,
        vec![(steve.id, "You are banned from this server".to_owned())]
    );
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":6,"method":"bans/remove","params":[[{"name":"Steve"}]]}"#,
    );
    assert!(r.ends_with(r#""result":[]}"#), "{r}");

    // ip bans by address and by online player
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":7,"method":"ip_bans/add","params":[[{"ip":"10.1.1.1"},{"ip":"not an ip"},{"player":{"name":"Steve"}}]]}"#,
    );
    assert!(
        r.contains(r#""ip":"10.1.1.1""#) && !r.contains("not an ip"),
        "{r}"
    );
    assert_eq!(mock.state.lock().ip_bans.len(), 2);

    // kicking: the player is removed and disconnected with the default message
    mock.state.lock().online.push(ManagementPlayer {
        profile: steve.clone(),
        ip_address: "10.1.1.1".into(),
    });
    let r = call(
        &d,
        r#"{"jsonrpc":"2.0","id":8,"method":"players/kick","params":[[{"player":{"name":"Steve"},"message":{"literal":"bye"}}]]}"#,
    );
    assert_eq!(r, r#"{"jsonrpc":"2.0","id":8,"result":[{"name":"Steve"}]}"#);
    assert!(
        mock.state
            .lock()
            .disconnected
            .iter()
            .any(|(_, m)| m == "bye")
    );

    // settings: set returns the value read back; invalid enum names are rejected
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":9,"method":"serversettings/motd/set","params":["hello"]}"#
        ),
        r#"{"jsonrpc":"2.0","id":9,"result":"hello"}"#
    );
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":10,"method":"serversettings/difficulty/set","params":["hard"]}"#
        ),
        r#"{"jsonrpc":"2.0","id":10,"result":"hard"}"#
    );
    assert!(call(&d, r#"{"jsonrpc":"2.0","id":11,"method":"serversettings/difficulty/set","params":["nope"]}"#).contains("-32602"));
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":12,"method":"serversettings/operator_user_permission_level"}"#
        ),
        r#"{"jsonrpc":"2.0","id":12,"result":4}"#
    );

    // server state and game rules
    assert!(
        call(&d, r#"{"jsonrpc":"2.0","id":13,"method":"server/status"}"#)
            .contains(r#""started":true,"version":{"name":"26.3","protocol":777}"#)
    );
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":14,"method":"server/save","params":[true]}"#
        ),
        r#"{"jsonrpc":"2.0","id":14,"result":true}"#
    );
    assert_eq!(
        call(
            &d,
            r#"{"jsonrpc":"2.0","id":15,"method":"gamerules/update","params":[{"key":"advance_time","value":false}]}"#
        ),
        r#"{"jsonrpc":"2.0","id":15,"result":{"key":"minecraft:advance_time","type":"boolean","value":false}}"#
    );
    assert!(call(&d, r#"{"jsonrpc":"2.0","id":16,"method":"gamerules/update","params":[{"key":"advance_time","value":3}]}"#).contains("Unknown or mistyped rule"));

    // batches answer every request that has an id, in order
    let r = call(
        &d,
        r#"[{"jsonrpc":"2.0","id":1,"method":"players"},{"jsonrpc":"2.0","method":"players"},{"jsonrpc":"2.0","id":2,"method":"nope"}]"#,
    );
    assert!(
        r.starts_with(r#"[{"jsonrpc":"2.0","id":1,"result":["#)
            && r.contains(r#"{"jsonrpc":"2.0","id":2,"error""#),
        "{r}"
    );
    assert_eq!(call(&d, "[]"), "[]");
    assert_eq!(
        d.handle_text("[1]", ClientInfo { connection_id: 1 }),
        Outbound::Close
    );
}

#[test]
fn websocket_round_trip() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    const SECRET: &str = "abcdefghijABCDEFGHIJ0123456789abcdefghij";
    let mock = Mock::new();
    let auth = AuthenticationHandler::new(SECRET.to_owned(), "https://ui.example");
    let mut server = ManagementServer::new("127.0.0.1", 0, auth).unwrap();
    let service: Arc<dyn NotificationService> = Arc::new(JsonRpcNotificationService::new(
        mock.clone(),
        server.connections(),
    ));
    server.start(mock, None).unwrap();
    let url = format!("ws://127.0.0.1:{}/", server.get_port());
    let connections = server.connections();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        // no key: 401 with the JSON body
        let err = tokio_tungstenite::connect_async(url.clone()).await.unwrap_err();
        match err {
            tokio_tungstenite::tungstenite::Error::Http(resp) => {
                assert_eq!(resp.status(), 401);
                assert_eq!(String::from_utf8(resp.body().clone().unwrap()).unwrap(), r#"{"error":"Unauthorized","message":"Missing API key"}"#);
            }
            other => panic!("unexpected {other:?}"),
        }
        // bearer key
        let mut request = url.clone().into_client_request().unwrap();
        request.headers_mut().insert("Authorization", format!("Bearer {SECRET}").parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        ws.send(Message::text(r#"{"jsonrpc":"2.0","id":1,"method":"players"}"#)).await.unwrap();
        let Message::Text(reply) = ws.next().await.unwrap().unwrap() else { panic!("text expected") };
        assert!(reply.as_str().starts_with(r#"{"jsonrpc":"2.0","id":1,"result":[{"id":""#), "{reply}");

        // sub-protocol key needs an allowed origin and is echoed back
        let mut request = url.clone().into_client_request().unwrap();
        request.headers_mut().insert("Sec-WebSocket-Protocol", format!("minecraft-v1,{SECRET}").parse().unwrap());
        request.headers_mut().insert("Origin", "https://ui.example".parse().unwrap());
        let (_, response) = tokio_tungstenite::connect_async(request).await.unwrap();
        assert_eq!(response.headers().get("Sec-WebSocket-Protocol").unwrap(), "minecraft-v1");

        // notifications reach every open connection
        service.player_joined(&NameAndId::create_offline("Alex"));
        let Message::Text(note) = ws.next().await.unwrap().unwrap() else { panic!("text expected") };
        assert!(note.as_str().starts_with(r#"{"jsonrpc":"2.0","method":"minecraft:notification/players/joined","params":[{"id":""#), "{note}");
        assert!(!connections.is_empty());
    });
    server.stop();
}

/// TLS with a keystore made by the JDK's `keytool` (`fixtures/dedicated/keystore.p12`, password `changeit`).
#[test]
fn tls_websocket_with_pkcs12_keystore() {
    use futures_util::{SinkExt, StreamExt};
    use mc_game::server::jsonrpc::security::create_tls_acceptor;
    use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName};
    use tokio_rustls::rustls::{ClientConfig, RootCertStore};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let keystore = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../mc-game/tests/fixtures/dedicated/keystore.p12"
    );
    assert!(create_tls_acceptor(keystore, "wrong").is_err());
    assert!(
        create_tls_acceptor("", "changeit")
            .err()
            .unwrap()
            .contains("keystore is not configured")
    );
    assert!(
        create_tls_acceptor("/nonexistent.p12", "changeit")
            .err()
            .unwrap()
            .contains("does not exist")
    );
    let acceptor = create_tls_acceptor(keystore, "changeit").expect("keystore loads");

    const SECRET: &str = "abcdefghijABCDEFGHIJ0123456789abcdefghij";
    let mock = Mock::new();
    let mut server = ManagementServer::new(
        "127.0.0.1",
        0,
        AuthenticationHandler::new(SECRET.to_owned(), ""),
    )
    .unwrap();
    server.start(mock, Some(acceptor)).unwrap();
    let port = server.get_port();

    let p12 =
        p12_keystore::KeyStore::from_pkcs12(&std::fs::read(keystore).unwrap(), "changeit").unwrap();
    let (_, chain) = p12.private_key_chain().unwrap();
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(chain.chain()[0].as_der().to_vec()))
        .unwrap();
    let client_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port as u16))
            .await
            .unwrap();
        let tls = connector
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
            .expect("TLS handshake");
        let mut request = format!("wss://localhost:{port}/")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {SECRET}").parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::client_async(request, tls).await.unwrap();
        ws.send(Message::text(
            r#"{"jsonrpc":"2.0","id":1,"method":"server/status"}"#,
        ))
        .await
        .unwrap();
        let Message::Text(reply) = ws.next().await.unwrap().unwrap() else {
            panic!("text expected")
        };
        assert!(reply.as_str().contains(r#""started":true"#), "{reply}");
    });
    server.stop();
}
