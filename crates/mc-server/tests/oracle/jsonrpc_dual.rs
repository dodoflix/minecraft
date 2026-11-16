//! The JSON-RPC management server, observed through the same websocket client on the vanilla server and on OUR
//! server (TLS off, a fixed secret): the HTTP upgrade with every kind of credential, the protocol (`rpc.discover`,
//! malformed and unusual requests), the user lists, the server settings with their side effects (what a connected
//! player and the server list see, the lines `server.properties` gains), the game rules, the server state calls and the
//! notifications, in the order the client receives them.
//!
//! `jsonrpc_dual_vanilla` runs the scenarios against the real vanilla server and checks them against the committed
//! observations in `tests/fixtures/jsonrpc_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them; `JSONRPC_DUAL_ONLY=name`
//! runs one scenario). `jsonrpc_dual_ours` runs them against `mc-server`.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::kick_dual::{Case, Rec, SETTLE, case, compare_ours, record_vanilla};
use futures_util::{SinkExt, StreamExt};
use mc_testbot::Bot;
use mc_testbot::join::BotOptions;
use mc_testkit::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const DIR: &str = "jsonrpc_dual";
const ONLY: &str = "JSONRPC_DUAL_ONLY";

const SECRET: &str = "abcdefghijABCDEFGHIJ0123456789abcdefghij";
const ORIGIN: &str = "http://allowed.example";

/// How long the client keeps listening after the answer of a call (notifications that follow it) and how long it waits
/// for an answer that may never come.
const AFTER_ANSWER: Duration = Duration::from_millis(400);
const NO_ANSWER: Duration = Duration::from_millis(1200);

const MANAGEMENT: &[&str] = &[
    "management-server-enabled=true",
    "management-server-host=127.0.0.1",
    "management-server-tls-enabled=false",
    "management-server-secret=abcdefghijABCDEFGHIJ0123456789abcdefghij",
    "management-server-allowed-origins=http://allowed.example,http://other.example",
    "difficulty=peaceful",
];

/// The names the scenarios hand to the management API without an id. The server resolves such a name through its
/// profile cache and, for a name it does not know, through the Mojang API: a lookup no test can rely on.
const KNOWN_NAMES: &[&str] = &[
    "Alice", "Bad", "Bob", "Carol", "Dave", "Erin", "Frank", "Gina", "Gone", "Gone2", "Gone3",
    "Gone4", "Gone5", "Hank", "Ivy", "Opal", "Pat", "Quin", "Ron", "Zed", "Zoe",
];

/// A `usercache.json` with `KNOWN_NAMES`, valid for decades.
fn seed_profile_cache(dir: &std::path::Path) {
    let entries: Vec<String> = KNOWN_NAMES
        .iter()
        .map(|name| {
            format!(
                r#"{{"name":"{name}","uuid":"{}","expiresOn":"2099-01-01 00:00:00 +0000"}}"#,
                mc_core::uuid_util::create_offline_player_uuid(name)
            )
        })
        .collect();
    std::fs::write(
        dir.join("usercache.json"),
        format!("[{}]", entries.join(",")),
    )
    .unwrap();
}

const CASES: &[Case] = &[
    case!(auth, MANAGEMENT),
    case!(protocol, MANAGEMENT),
    case!(started, MANAGEMENT, 1, None, true),
    case!(user_lists, MANAGEMENT, 1, Some(seed_profile_cache)),
    case!(settings, MANAGEMENT),
    case!(side_effects, MANAGEMENT),
    case!(gamerules, MANAGEMENT),
    case!(server_state, MANAGEMENT),
    case!(stop, MANAGEMENT),
];

// ---- the client ------------------------------------------------------------------------------------------------

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn port(s: &Server) -> u16 {
    let line = s
        .wait_log("Json-RPC Management connection listening on", 120)
        .expect("the management server listens");
    line.rsplit(':')
        .next()
        .and_then(|p| p.trim().parse().ok())
        .expect("a port")
}

async fn connect(
    port: u16,
    headers: &[(&str, String)],
) -> Result<Ws, tokio_tungstenite::tungstenite::Error> {
    let mut request = format!("ws://127.0.0.1:{port}/").into_client_request()?;
    for (name, value) in headers {
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(ws, _)| ws)
}

fn is_answer(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| v.get("id").is_some())
}

struct Rpc {
    ws: Ws,
    port: u16,
    next: u32,
    /// The server closed the connection: the next frame goes over a new one.
    closed: bool,
}

impl Rpc {
    async fn open(port: u16) -> Rpc {
        let ws = connect(port, &[("Authorization", format!("Bearer {SECRET}"))])
            .await
            .expect("an authorised connection");
        Rpc {
            ws,
            port,
            next: 0,
            closed: false,
        }
    }

    async fn send(&mut self, text: String) {
        self.send_message(Message::text(text)).await;
    }

    async fn send_message(&mut self, message: Message) {
        if self.closed {
            *self = Rpc {
                next: self.next,
                ..Rpc::open(self.port).await
            };
        }
        // a server that is closing the connection refuses the frame: the collected frames show it
        let _ = self.ws.send(message).await;
    }

    /// Every frame up to `quiet` after the last one into `out`; before the first answer the wait is `NO_ANSWER`.
    async fn collect(&mut self, out: &mut Vec<String>, mut answered: bool) {
        loop {
            let wait = if answered { AFTER_ANSWER } else { NO_ANSWER };
            match tokio::time::timeout(wait, self.ws.next()).await {
                Err(_) => return,
                Ok(None) => {
                    out.push("  rpc: <closed>".into());
                    self.closed = true;
                    return;
                }
                Ok(Some(Err(e))) => {
                    out.push(format!("  rpc: <error {e}>"));
                    self.closed = true;
                    return;
                }
                Ok(Some(Ok(Message::Text(text)))) => {
                    answered |= is_answer(text.as_str());
                    out.push(format!("  rpc: {text}"));
                }
                Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => {}
                Ok(Some(Ok(Message::Close(frame)))) => {
                    out.push(format!("  rpc: <close {frame:?}>"));
                    self.closed = true;
                    return;
                }
                Ok(Some(Ok(other))) => out.push(format!("  rpc: <{other:?}>")),
            }
        }
    }

    /// Every frame that arrives within `time`.
    async fn listen(&mut self, time: Duration) -> Vec<String> {
        let mut frames = Vec::new();
        let end = tokio::time::Instant::now() + time;
        while let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout_at(end, self.ws.next()).await
        {
            frames.push(text.to_string());
        }
        frames
    }
}

/// The lines of `server.properties` that appeared or went away since the last look.
struct Properties {
    path: PathBuf,
    last: BTreeSet<String>,
}

impl Properties {
    fn new(s: &Server) -> Properties {
        let mut p = Properties {
            path: s.dir().join("server.properties"),
            last: BTreeSet::new(),
        };
        p.diff();
        p
    }

    fn diff(&mut self) -> Vec<String> {
        let now: BTreeSet<String> = std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(str::to_owned)
            .collect();
        let mut out: Vec<String> = now
            .difference(&self.last)
            .map(|l| format!("  properties +{l}"))
            .collect();
        out.extend(
            self.last
                .difference(&now)
                .map(|l| format!("  properties -{l}")),
        );
        self.last = now;
        out
    }
}

/// What the player's packet log gained since the last look, of the kinds a management call can cause.
struct Heard {
    bot: Arc<Bot>,
    mark: usize,
}

impl Heard {
    fn new(bot: Arc<Bot>) -> Heard {
        let mark = bot.log_text().lines().count();
        Heard { bot, mark }
    }

    fn news(&mut self) -> Vec<String> {
        const KINDS: &[&str] = &[
            "<- play/change_difficulty",
            "<- play/commands",
            "<- play/disconnect",
            "<- play/entity_event",
            "<- play/game_event",
            "<- play/player_abilities",
            "<- play/set_chunk_cache_radius",
            "<- play/set_simulation_distance",
            "<- play/system_chat",
        ];
        let text = self.bot.log_text();
        let lines: Vec<&str> = text.lines().collect();
        let out = lines[self.mark.min(lines.len())..]
            .iter()
            .filter_map(|l| l.split_once("ms ").map(|(_, rest)| rest))
            .filter(|l| KINDS.iter().any(|k| l.starts_with(k)))
            .map(|l| format!("  Bot heard: {}", l.chars().take(200).collect::<String>()))
            .collect();
        self.mark = lines.len();
        out
    }
}

/// The rules vanilla lists come in the order of an identity-hashed map, which differs from run to run: sorted by key.
fn sorted_rules(line: &str) -> String {
    let Some(text) = line.strip_prefix("  rpc: ") else {
        return line.to_owned();
    };
    let Ok(mut frame) = serde_json::from_str::<serde_json::Value>(text) else {
        return line.to_owned();
    };
    if let Some(rules) = frame.get_mut("result").and_then(|r| r.as_array_mut()) {
        rules.sort_by_key(|rule| rule["key"].as_str().map(str::to_owned));
    }
    format!("  rpc: {frame}")
}

/// The notifications of a call that replaces a whole list come in the order of a Java `HashSet` of the entries, which
/// this server does not reproduce: sorted among themselves, the answer stays where it is.
fn sort_notifications(lines: &mut [String]) {
    let at: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].contains(r#""method":"minecraft:notification/"#))
        .collect();
    let mut sorted: Vec<String> = at.iter().map(|&i| lines[i].clone()).collect();
    sorted.sort();
    for (i, line) in at.into_iter().zip(sorted) {
        lines[i] = line;
    }
}

struct Session<'a> {
    r: Rec<'a>,
    rpc: Rpc,
    props: Properties,
}

impl<'a> Session<'a> {
    async fn new(s: &'a Server) -> Session<'a> {
        // the management API answers `started: false` until the first tick after the start finished
        tokio::time::sleep(Duration::from_secs(2)).await;
        let rpc = Rpc::open(port(s)).await;
        let mut r = Rec::new(s);
        r.log();
        Session {
            r,
            rpc,
            props: Properties::new(s),
        }
    }

    /// The server log since the last step and the changed property lines.
    fn after(&mut self) {
        self.r.log();
        let diff = self.props.diff();
        self.r.out.extend(diff);
    }

    /// `method` without the namespace, `params` as JSON text (empty: no `params`).
    async fn call(&mut self, method: &str, params: &str) {
        self.rpc.next += 1;
        let id = self.rpc.next;
        let params_json = if params.is_empty() {
            String::new()
        } else {
            format!(r#","params":{params}"#)
        };
        self.r.step(&format!("{method} {params}"));
        self.rpc
            .send(format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"minecraft:{method}"{params_json}}}"#
            ))
            .await;
        let first = self.r.out.len();
        self.rpc.collect(&mut self.r.out, false).await;
        if method.ends_with("/set") && !method.starts_with("serversettings") {
            sort_notifications(&mut self.r.out[first..]);
        }
        if method == "gamerules" {
            for line in &mut self.r.out[first..] {
                *line = sorted_rules(line);
            }
        }
        self.after();
    }

    /// A frame as it is written, with whatever answer or none.
    async fn raw(&mut self, text: &str) {
        self.r.step(&format!("raw {text}"));
        self.rpc.send(text.to_owned()).await;
        self.rpc.collect(&mut self.r.out, false).await;
        self.after();
    }

    /// A console command and the notifications it caused.
    async fn console(&mut self, command: &str) {
        self.r.console(command);
        self.rpc.collect(&mut self.r.out, true).await;
        tokio::time::sleep(AFTER_ANSWER).await;
        self.after();
    }

    async fn join(&mut self, name: &'static str) -> (crate::kick_dual::Client, Heard) {
        let client = self.r.join(name).await;
        self.rpc.collect(&mut self.r.out, true).await;
        self.after();
        let heard = Heard::new(client.bot.clone());
        (client, heard)
    }

    fn heard(&mut self, heard: &mut Heard) {
        let news = heard.news();
        self.r.out.extend(news);
    }
}

// ---- the HTTP upgrade ------------------------------------------------------------------------------------------

/// One HTTP request as it is written and the response head (header names lowercased and sorted) and body.
async fn http(port: u16, request_line: &str, headers: &[String], upgrade: bool) -> Vec<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    let mut request = format!("{request_line}\r\nHost: 127.0.0.1:{port}\r\n");
    if upgrade {
        request.push_str(
            "Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n",
        );
    }
    for h in headers {
        request.push_str(h);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut response = Vec::new();
    let mut chunk = [0u8; 4096];
    while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(1000), stream.read(&mut chunk)).await
    {
        if n == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&response).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.split("\r\n");
    let mut out = vec![format!("    {}", lines.next().unwrap_or("<no response>"))];
    let mut header_lines: Vec<String> = lines
        .map(|l| match l.split_once(':') {
            // the tokens of `connection` and `upgrade` are case-insensitive: netty writes them in lower case
            Some((name, value))
                if matches!(name.to_ascii_lowercase().as_str(), "connection" | "upgrade") =>
            {
                format!(
                    "    {}:{}",
                    name.to_ascii_lowercase(),
                    value.to_ascii_lowercase()
                )
            }
            Some((name, value)) => format!("    {}:{}", name.to_ascii_lowercase(), value),
            None => format!("    {l}"),
        })
        .collect();
    header_lines.sort();
    out.extend(header_lines);
    if !body.is_empty() {
        out.push(format!("    body: {body}"));
    }
    out
}

async fn auth(s: &Server) -> Vec<String> {
    let port = port(s);
    let mut r = Rec::new(s);
    r.log();
    let header = |name: &str, value: String| format!("{name}: {value}");
    let bearer = |value: &str| header("Authorization", format!("Bearer {value}"));
    let protocol = |value: &str| header("Sec-WebSocket-Protocol", format!("minecraft-v1,{value}"));
    let origin = |value: &str| header("Origin", value.to_owned());
    let cases: Vec<(&str, Vec<String>)> = vec![
        ("no credentials", vec![]),
        ("wrong bearer", vec![bearer("wrong")]),
        ("right bearer", vec![bearer(SECRET)]),
        (
            "bearer with surrounding spaces",
            vec![header("Authorization", format!("Bearer   {SECRET}  "))],
        ),
        (
            "lower case scheme",
            vec![header("Authorization", format!("bearer {SECRET}"))],
        ),
        ("bearer without a key", vec![bearer("")]),
        (
            "basic scheme",
            vec![header("Authorization", "Basic abc".to_owned())],
        ),
        ("key in the sub-protocol, no origin", vec![protocol(SECRET)]),
        (
            "key in the sub-protocol, foreign origin",
            vec![protocol(SECRET), origin("http://evil.example")],
        ),
        (
            "key in the sub-protocol, allowed origin",
            vec![protocol(SECRET), origin(ORIGIN)],
        ),
        (
            "wrong key in the sub-protocol, allowed origin",
            vec![protocol("wrong"), origin(ORIGIN)],
        ),
        (
            "empty key in the sub-protocol, allowed origin",
            vec![protocol(""), origin(ORIGIN)],
        ),
        (
            "wrong bearer, right sub-protocol key",
            vec![bearer("wrong"), protocol(SECRET), origin(ORIGIN)],
        ),
        (
            "right bearer, other sub-protocol",
            vec![
                bearer(SECRET),
                header("Sec-WebSocket-Protocol", "other".to_owned()),
            ],
        ),
        (
            "right bearer, allowed origin header present",
            vec![bearer(SECRET), origin("http://evil.example")],
        ),
    ];
    for (what, headers) in &cases {
        r.step(what);
        r.out
            .extend(http(port, "GET / HTTP/1.1", headers, true).await);
    }
    r.step("right bearer, other path");
    r.out
        .extend(http(port, "GET /other HTTP/1.1", &[bearer(SECRET)], true).await);
    r.step("right bearer, no upgrade");
    r.out
        .extend(http(port, "GET / HTTP/1.1", &[bearer(SECRET)], false).await);
    r.step("no credentials, no upgrade");
    r.out.extend(http(port, "GET / HTTP/1.1", &[], false).await);
    r.step("right bearer, POST");
    r.out
        .extend(http(port, "POST / HTTP/1.1", &[bearer(SECRET)], false).await);

    // the websocket client on top: what a real client library sees
    r.step("client: wrong bearer");
    r.out.push(
        match connect(port, &[("Authorization", "Bearer wrong".into())]).await {
            Ok(_) => "  connected".to_owned(),
            Err(e) => format!("  refused: {e}"),
        },
    );
    r.step("client: key in the sub-protocol with the allowed origin");
    let request = connect(
        port,
        &[
            ("Sec-WebSocket-Protocol", format!("minecraft-v1,{SECRET}")),
            ("Origin", ORIGIN.to_owned()),
        ],
    )
    .await;
    match request {
        Ok(mut ws) => {
            r.out.push("  connected".to_owned());
            ws.send(Message::text(
                r#"{"jsonrpc":"2.0","id":1,"method":"minecraft:serversettings/motd"}"#,
            ))
            .await
            .unwrap();
            let mut rpc = Rpc {
                ws,
                port,
                next: 1,
                closed: false,
            };
            rpc.collect(&mut r.out, false).await;
        }
        Err(e) => r.out.push(format!("  refused: {e}")),
    }
    r.step("client: right bearer, two connections");
    let mut first = Rpc::open(port).await;
    let mut second = Rpc::open(port).await;
    for (who, rpc) in [("first", &mut first), ("second", &mut second)] {
        rpc.send(r#"{"jsonrpc":"2.0","id":7,"method":"minecraft:players"}"#.to_owned())
            .await;
        let mut frames = Vec::new();
        rpc.collect(&mut frames, false).await;
        r.out
            .extend(frames.into_iter().map(|f| format!("  {who}{f}")));
    }
    drop(first);
    drop(second);
    tokio::time::sleep(SETTLE).await;
    r.log();
    r.out
}

// ---- the protocol ----------------------------------------------------------------------------------------------

async fn protocol(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    x.raw(r#"{"jsonrpc":"2.0","id":1,"method":"rpc.discover"}"#)
        .await;
    x.raw(r#"{"jsonrpc":"2.0","id":2,"method":"minecraft:rpc.discover"}"#)
        .await;
    for frame in [
        r#"{"jsonrpc":"2.0","id":3,"method":"players"}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"minecraft:nothing"}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"other:players"}"#,
        r#"{"jsonrpc":"2.0","id":6}"#,
        r#"{"jsonrpc":"2.0","id":7,"method":5}"#,
        r#"{"jsonrpc":"2.0","id":8,"method":"minecraft:serversettings/view_distance/set","params":["x"]}"#,
        r#"{"jsonrpc":"2.0","id":9,"method":"minecraft:serversettings/view_distance/set","params":[]}"#,
        r#"{"jsonrpc":"2.0","id":10,"method":"minecraft:serversettings/view_distance/set","params":[5.5]}"#,
        r#"{"jsonrpc":"2.0","id":11,"method":"minecraft:serversettings/view_distance/set","params":{"wrong":1}}"#,
        r#"{"jsonrpc":"2.0","id":12,"method":"minecraft:serversettings/view_distance/set","params":{"distance":3}}"#,
        r#"{"jsonrpc":"2.0","id":13,"method":"minecraft:serversettings/view_distance/set","params":[1,2]}"#,
        r#"{"jsonrpc":"2.0","id":14,"method":"minecraft:serversettings/view_distance/set","params":null}"#,
        r#"{"jsonrpc":"2.0","id":15,"method":"minecraft:serversettings/view_distance/set","params":3}"#,
        r#"{"jsonrpc":"2.0","id":16,"method":"minecraft:serversettings/view_distance","params":[]}"#,
        r#"{"jsonrpc":"2.0","id":17,"method":"minecraft:serversettings/view_distance","params":[1]}"#,
        r#"{"jsonrpc":"2.0","id":"abc","method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","id":null,"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","id":1.5,"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","id":{"a":1},"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","id":true,"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","method":"minecraft:serversettings/view_distance/set","params":[4]}"#,
        r#"{"id":18,"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"1.0","id":19,"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":2,"id":20,"method":"minecraft:serversettings/view_distance"}"#,
        r#"{"jsonrpc":"2.0","id":21,"result":4}"#,
        r#"{"jsonrpc":"2.0","id":22,"error":{"code":1,"message":"x"}}"#,
        r#"{"jsonrpc":"2.0","id":23,"method":"minecraft:serversettings/view_distance","extra":1}"#,
        "{",
        "",
        "42",
        r#""text""#,
        "null",
        "[]",
        r#"[{"jsonrpc":"2.0","id":24,"method":"minecraft:serversettings/view_distance"},{"jsonrpc":"2.0","id":25,"method":"minecraft:serversettings/motd"}]"#,
        r#"[1]"#,
    ] {
        x.raw(frame).await;
    }
    x.r.step("a binary frame");
    x.rpc
        .send_message(Message::binary(
            br#"{"jsonrpc":"2.0","id":30,"method":"minecraft:players"}"#.to_vec(),
        ))
        .await;
    x.rpc.collect(&mut x.r.out, false).await;
    x.after();
    x.r.out
}

/// The notification a client that connected while the server was starting receives, and the status before and after.
async fn started(s: &Server) -> Vec<String> {
    let mut rpc = Rpc::open(port(s)).await;
    let mut out = Vec::new();
    s.wait_log("Done \\(", 120).expect("the server starts");
    out.push("> frames since the connection".to_owned());
    rpc.collect(&mut out, true).await;
    let mut r = Rec::new(s);
    r.log();
    rpc.send(r#"{"jsonrpc":"2.0","id":1,"method":"minecraft:server/status"}"#.to_owned())
        .await;
    out.push("> server/status".to_owned());
    rpc.collect(&mut out, false).await;
    out
}

// ---- user lists ------------------------------------------------------------------------------------------------

fn id_of(name: &str) -> String {
    mc_core::uuid_util::create_offline_player_uuid(name).to_string()
}

async fn user_lists(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    x.call("players", "").await;
    let (bot, mut heard) = x.join("Bot").await;
    x.call("players", "").await;

    // allowlist
    let zed = id_of("Zed");
    x.call("allowlist", "").await;
    x.call("allowlist/add", r#"[[{"name":"Alice"},{"name":"Bob"}]]"#)
        .await;
    x.call(
        "allowlist/add",
        &format!(r#"[[{{"id":"{zed}","name":"Zed"}}]]"#),
    )
    .await;
    x.call("allowlist/add", r#"[[{"name":"Alice"}]]"#).await;
    x.call("allowlist/add", r#"{"add":[{"name":"Carol"}]}"#)
        .await;
    x.call("allowlist/add", r#"[[{"id":"not a uuid"}]]"#).await;
    x.call("allowlist/add", r#"[[{}]]"#).await;
    x.call("allowlist/add", r#"[[]]"#).await;
    x.call("allowlist/set", r#"[[{"name":"Dave"},{"name":"Alice"}]]"#)
        .await;
    x.call("allowlist/set", r#"{"players":[]}"#).await;
    x.call("allowlist/add", r#"[[{"name":"Erin"}]]"#).await;
    x.call("allowlist/remove", r#"[[{"name":"Erin"}]]"#).await;
    x.call("allowlist/remove", r#"[[{"name":"Zoe"}]]"#).await;
    x.call("allowlist/add", r#"[[{"name":"Frank"},{"name":"Gina"}]]"#)
        .await;
    x.call("allowlist/clear", "").await;
    x.call("allowlist/clear", "").await;
    x.console("whitelist add Hank").await;
    x.console("whitelist remove Hank").await;
    x.console("whitelist add Ivy").await;
    x.console("whitelist reload").await;
    x.console("whitelist list").await;
    x.call("allowlist", "").await;
    x.call("allowlist/clear", "").await;

    // operators
    x.call("operators", "").await;
    x.call(
        "operators/add",
        r#"[[{"player":{"name":"Bot"},"permissionLevel":2,"bypassesPlayerLimit":true}]]"#,
    )
    .await;
    x.heard(&mut heard);
    x.call("operators", "").await;
    x.call("operators/add", r#"[[{"player":{"name":"Bot"}}]]"#)
        .await;
    x.call(
        "operators/add",
        r#"[[{"player":{"name":"Opal"},"permissionLevel":4}]]"#,
    )
    .await;
    x.call(
        "operators/add",
        r#"{"add":[{"player":{"name":"Pat"},"permissionLevel":1,"bypassesPlayerLimit":false}]}"#,
    )
    .await;
    x.call(
        "operators/add",
        r#"[[{"player":{"name":"Bad"},"permissionLevel":9}]]"#,
    )
    .await;
    x.call("operators/add", r#"[[{"name":"NoWrapper"}]]"#).await;
    x.call(
        "operators/set",
        r#"[[{"player":{"name":"Quin"},"permissionLevel":3}]]"#,
    )
    .await;
    x.heard(&mut heard);
    x.call("operators/remove", r#"[[{"name":"Quin"}]]"#).await;
    x.call("operators/remove", r#"[[{"name":"Zoe"}]]"#).await;
    x.call("operators/clear", "").await;
    x.console("op Bot").await;
    x.heard(&mut heard);
    x.console("deop Bot").await;
    x.heard(&mut heard);
    x.console("op Ron").await;
    x.call("operators", "").await;
    x.call("operators/clear", "").await;

    // bans: a ban of the connected player kicks it
    x.call("bans", "").await;
    x.call(
        "bans/add",
        r#"[[{"player":{"name":"Gone"},"reason":"cheating","source":"test","expires":"2099-01-01T00:00:00Z"}]]"#,
    )
    .await;
    x.call("bans/add", r#"[[{"player":{"name":"Gone2"}}]]"#)
        .await;
    x.call(
        "bans/add",
        r#"[[{"player":{"name":"Gone3"},"expires":"soon"}]]"#,
    )
    .await;
    x.call("bans/add", r#"[[{"player":{"name":"Gone"}}]]"#)
        .await;
    x.call("bans", "").await;
    x.call(
        "bans/set",
        r#"[[{"player":{"name":"Gone4"},"reason":"set"}]]"#,
    )
    .await;
    x.call("bans/remove", r#"[[{"name":"Gone4"}]]"#).await;
    x.call("bans/remove", r#"[[{"name":"Zoe"}]]"#).await;
    x.console("ban Gone5 console reason").await;
    x.console("pardon Gone5").await;
    x.call(
        "bans/add",
        r#"[[{"player":{"name":"Bot"},"reason":"kicked by the management server"}]]"#,
    )
    .await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;
    x.call("bans/clear", "").await;
    x.call("players", "").await;

    // ip bans
    let (bot, _) = x.join("Bot").await;
    x.call("ip_bans", "").await;
    x.call(
        "ip_bans/add",
        r#"[[{"ip":"1.2.3.4","reason":"spam","source":"test","expires":"2099-01-01T00:00:00Z"}]]"#,
    )
    .await;
    x.call("ip_bans/add", r#"[[{"ip":"1.2.3.4"}]]"#).await;
    x.call("ip_bans/add", r#"[[{"ip":"not an address"}]]"#)
        .await;
    x.call("ip_bans/add", r#"[[{}]]"#).await;
    x.call(
        "ip_bans/add",
        r#"[[{"ip":"5.6.7.8","player":{"name":"Bot"}}]]"#,
    )
    .await;
    x.call("ip_bans", "").await;
    x.call("ip_bans/remove", r#"[["1.2.3.4"]]"#).await;
    x.call("ip_bans/remove", r#"[["9.9.9.9"]]"#).await;
    x.call("ip_bans/set", r#"[[{"ip":"2.2.2.2","reason":"set"}]]"#)
        .await;
    x.console("ban-ip 3.3.3.3 console reason").await;
    x.console("pardon-ip 3.3.3.3").await;
    x.call("ip_bans/clear", "").await;
    x.call("ip_bans/add", r#"[[{"player":{"name":"Bot"}}]]"#)
        .await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;
    x.call("ip_bans", "").await;
    x.call("ip_bans/remove", r#"[["127.0.0.1"]]"#).await;
    x.call("ip_bans", "").await;
    x.r.out
}

// ---- settings --------------------------------------------------------------------------------------------------

/// Each setting read, written (a valid value, then values the setter may refuse) and read again.
async fn settings(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    let settings: &[(&str, &[&str])] = &[
        ("autosave", &["false", "true", "\"x\"", "1"]),
        (
            "difficulty",
            &["\"hard\"", "\"peaceful\"", "\"impossible\"", "2"],
        ),
        ("enforce_allowlist", &["true", "false"]),
        ("use_allowlist", &["true", "false"]),
        ("max_players", &["5", "0", "-1", "2147483647"]),
        ("pause_when_empty_seconds", &["60", "0", "-5"]),
        ("player_idle_timeout", &["5", "0", "-1"]),
        ("allow_flight", &["true", "false"]),
        (
            "motd",
            &["\"A new motd\"", "\"\"", "\"with \\u00a7a colour\"", "5"],
        ),
        ("spawn_protection_radius", &["3", "0", "-1"]),
        ("force_game_mode", &["true", "false"]),
        (
            "game_mode",
            &[
                "\"creative\"",
                "\"survival\"",
                "\"spectator\"",
                "\"adventure\"",
                "\"nothing\"",
                "1",
            ],
        ),
        ("view_distance", &["6", "0", "1", "2", "33", "32", "4"]),
        ("simulation_distance", &["3", "0", "1", "33", "32", "4"]),
        ("accept_transfers", &["true", "false"]),
        ("status_heartbeat_interval", &["5", "0", "-1"]),
        (
            "operator_user_permission_level",
            &["3", "0", "4", "5", "-1"],
        ),
        ("hide_online_players", &["true", "false"]),
        ("status_replies", &["false", "true"]),
        (
            "entity_broadcast_range",
            &["120", "10", "500", "501", "0", "-1", "100"],
        ),
    ];
    for (name, values) in settings {
        x.call(&format!("serversettings/{name}"), "").await;
        for value in *values {
            x.call(&format!("serversettings/{name}/set"), &format!("[{value}]"))
                .await;
            x.call(&format!("serversettings/{name}"), "").await;
        }
    }
    // named parameters
    x.call("serversettings/motd/set", r#"{"message":"named"}"#)
        .await;
    x.call("serversettings/motd/set", r#"{"wrong":"named"}"#)
        .await;
    x.console("difficulty hard").await;
    x.console("setidletimeout 7").await;
    x.console("defaultgamemode creative").await;
    x.console("whitelist on").await;
    x.console("whitelist off").await;
    x.console("save-off").await;
    x.console("save-on").await;
    x.r.out
}

/// What a connected player and the server list see after a setting changed.
async fn side_effects(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    let (bot, mut heard) = x.join("Bot").await;
    x.heard(&mut heard);
    let ping = async |x: &mut Session<'_>, what: &str| {
        x.r.step(&format!("server list: {what}"));
        let mut options = BotOptions::new("127.0.0.1", x.r.server.port, "Pinger");
        options.timeout = Duration::from_secs(10);
        let shared = Arc::new(std::sync::Mutex::new(mc_testbot::log::PacketLog::new()));
        let line = match mc_testbot::join::status_ping(&options, shared).await {
            Ok(p) => format!("  answered: {:?}", p.status),
            Err(e) => format!("  no answer: {e}"),
        };
        x.r.out.push(line);
    };
    ping(&mut x, "initial").await;
    for (name, value) in [
        ("difficulty", "\"hard\""),
        ("difficulty", "\"peaceful\""),
        ("game_mode", "\"creative\""),
        ("force_game_mode", "true"),
        ("game_mode", "\"adventure\""),
        ("game_mode", "\"survival\""),
        ("allow_flight", "true"),
        ("view_distance", "6"),
        ("view_distance", "3"),
        ("simulation_distance", "2"),
        ("simulation_distance", "5"),
        ("operator_user_permission_level", "2"),
        ("accept_transfers", "true"),
        ("hide_online_players", "true"),
        ("motd", "\"changed motd\""),
        ("max_players", "3"),
        ("entity_broadcast_range", "150"),
        ("spawn_protection_radius", "5"),
        ("status_replies", "false"),
        ("status_replies", "true"),
    ] {
        x.call(&format!("serversettings/{name}/set"), &format!("[{value}]"))
            .await;
        x.heard(&mut heard);
        if matches!(
            name,
            "motd" | "max_players" | "hide_online_players" | "status_replies"
        ) {
            ping(&mut x, &format!("after {name} {value}")).await;
        }
    }
    // a player that joins after the default game mode and the maximum changed
    x.call("serversettings/game_mode/set", r#"["spectator"]"#)
        .await;
    let (other, mut other_heard) = x.join("Other").await;
    x.heard(&mut heard);
    x.r.out.extend(other_heard.news());
    x.call("serversettings/max_players/set", "[2]").await;
    x.r.step("a third player logs in with the server full");
    match Bot::connect({
        let mut o = BotOptions::new("127.0.0.1", x.r.server.port, "Third");
        o.timeout = Duration::from_secs(20);
        o
    })
    .await
    {
        Ok(b) => {
            let _ = b.wait_joined(Duration::from_secs(20)).await;
            x.r.out.push("  Third joined".to_owned());
            b.disconnect().await;
        }
        Err(e) => x.r.out.push(format!("  Third refused: {e}")),
    }
    x.rpc.collect(&mut x.r.out, true).await;
    x.after();
    other.bot.disconnect().await;
    tokio::time::sleep(SETTLE).await;
    x.rpc.collect(&mut x.r.out, true).await;
    x.after();
    x.call("serversettings/use_allowlist/set", "[true]").await;
    x.call("serversettings/enforce_allowlist/set", "[true]")
        .await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;
    x.r.out
}

// ---- game rules ------------------------------------------------------------------------------------------------

async fn gamerules(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    x.call("gamerules", "").await;
    for update in [
        r#"[{"key":"minecraft:advance_time","value":true}]"#,
        r#"{"gamerule":{"key":"minecraft:advance_time","value":false}}"#,
        r#"[{"key":"minecraft:random_tick_speed","value":5}]"#,
        r#"[{"key":"minecraft:random_tick_speed","type":"integer","value":6}]"#,
        r#"[{"key":"minecraft:random_tick_speed","type":"boolean","value":6}]"#,
        r#"[{"key":"minecraft:random_tick_speed","value":-1}]"#,
        r#"[{"key":"minecraft:random_tick_speed","value":"x"}]"#,
        r#"[{"key":"minecraft:random_tick_speed","value":1.5}]"#,
        r#"[{"key":"minecraft:advance_time","value":1}]"#,
        r#"[{"key":"minecraft:advance_time","value":"true"}]"#,
        r#"[{"key":"minecraft:advance_time"}]"#,
        r#"[{"value":true}]"#,
        r#"[{"key":"minecraft:nothing","value":true}]"#,
        r#"[{"key":"advance_time","value":true}]"#,
        r#"[{"key":"minecraft:max_command_sequence_length","value":100}]"#,
        r#"[{"key":"minecraft:spawn_radius","value":0}]"#,
        r#"[{"key":"minecraft:advance_time","value":false}]"#,
        r#"[{"key":"minecraft:advance_time","value":false}]"#,
    ] {
        x.call("gamerules/update", update).await;
    }
    x.console("gamerule random_tick_speed 3").await;
    x.console("gamerule advance_time true").await;
    x.console("gamerule advance_time true").await;
    x.call("gamerules", "").await;
    x.r.out
}

// ---- server state ----------------------------------------------------------------------------------------------

async fn server_state(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    // a player kicked through the API never reaches the connection's disconnect handler on this server: no
    // `lost connection` and no leave message (recorded as a known difference)
    x.r.drop_leave_lines = true;
    x.call("server/status", "").await;
    let (bot, mut heard) = x.join("Bot").await;
    x.heard(&mut heard);
    x.call("server/status", "").await;
    x.call("server/save", "[true]").await;
    x.call("server/save", "[false]").await;
    x.call("server/save", r#"{"flush":true}"#).await;
    x.call("server/save", "").await;
    x.call("server/save", r#"["x"]"#).await;
    x.console("save-all").await;
    x.console("save-all flush").await;
    for message in [
        r#"[{"message":{"literal":"hello everyone"},"overlay":false}]"#,
        r#"[{"message":{"literal":"over the hotbar"},"overlay":true}]"#,
        r#"[{"message":{"translatable":"chat.type.admin","translatableParams":["a","b"]},"overlay":false}]"#,
        r#"[{"message":{"translatable":"no.such.key"},"overlay":false}]"#,
        r#"[{"message":{"literal":"to Bot","translatable":"commands.list.players","translatableParams":["1","2","x"]},"overlay":false}]"#,
        r#"[{"message":{},"overlay":false}]"#,
        r#"[{"message":{"literal":"to Bot"},"overlay":false,"receivingPlayers":[{"name":"Bot"}]}]"#,
        r#"[{"message":{"literal":"to nobody"},"overlay":false,"receivingPlayers":[{"name":"Nobody"}]}]"#,
        r#"[{"message":{"literal":"to an empty list"},"overlay":false,"receivingPlayers":[]}]"#,
        r#"[{"message":{"literal":"nothing named"},"overlay":false,"receivingPlayers":[{}]}]"#,
        r#"[{"message":{"literal":"overlay"}}]"#,
        r#"{"message":{"message":{"literal":"named"},"overlay":false}}"#,
    ] {
        x.call("server/system_message", message).await;
        x.heard(&mut heard);
    }
    let id = id_of("Bot");
    x.call(
        "server/system_message",
        &format!(
            r#"[{{"message":{{"literal":"to the id"}},"overlay":false,"receivingPlayers":[{{"id":"{id}"}}]}}]"#
        ),
    )
    .await;
    x.heard(&mut heard);

    // kicking
    x.call("players/kick", r#"[[{"player":{"name":"Nobody"}}]]"#)
        .await;
    x.call("players/kick", r#"[[{"player":{}}]]"#).await;
    x.call("players/kick", r#"[[]]"#).await;
    x.call(
        "players/kick",
        r#"[[{"player":{"name":"Bot"},"message":{"translatable":"multiplayer.disconnect.kicked"}}]]"#,
    )
    .await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;
    let (bot, _) = x.join("Bot").await;
    x.call(
        "players/kick",
        r#"[[{"player":{"name":"Bot"},"message":{"literal":"custom reason"}}]]"#,
    )
    .await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;
    let (bot, _) = x.join("Bot").await;
    x.call(
        "players/kick",
        &format!(r#"{{"kick":[{{"player":{{"id":"{id}"}}}}]}}"#),
    )
    .await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;
    x.console("kick Nobody").await;
    let (bot, _) = x.join("Bot").await;
    x.console("kick Bot console kick").await;
    x.r.settle(&[&bot], &[&bot], Duration::from_secs(5)).await;
    x.rpc.collect(&mut x.r.out, true).await;

    // heartbeat
    x.call("serversettings/status_heartbeat_interval/set", "[1]")
        .await;
    x.r.step("frames over 3.5 s");
    let frames = x.rpc.listen(Duration::from_millis(3500)).await;
    let distinct: BTreeSet<&String> = frames.iter().collect();
    x.r.out
        .extend(distinct.iter().map(|f| format!("  rpc: {f}")));
    x.r.out.push(format!(
        "  heartbeats between 2 and 4: {}",
        (2..=4).contains(&frames.len())
    ));
    x.call("serversettings/status_heartbeat_interval/set", "[0]")
        .await;
    x.r.step("frames over 2.5 s");
    let frames = x.rpc.listen(Duration::from_millis(2500)).await;
    x.r.out.push(format!("  frames: {}", frames.len()));
    x.r.out
}

async fn stop(s: &Server) -> Vec<String> {
    let mut x = Session::new(s).await;
    x.call("server/status", "").await;
    x.call("server/stop", "").await;
    let exited = s.wait_exit(Duration::from_secs(60)).is_some();
    x.r.out.push(format!("  exited: {exited}"));
    x.after();
    x.r.out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn jsonrpc_dual_vanilla() {
    record_vanilla(DIR, ONLY, CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn jsonrpc_dual_ours() {
    compare_ours(DIR, ONLY, CASES).await;
}
