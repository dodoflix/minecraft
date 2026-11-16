//! Every way a player leaves a server against their will, observed through the same bot script on the vanilla server and
//! on OUR server: the reason text the client receives, what the other players see (system chat, player info removal) and
//! the server log lines, in order, for `/stop`, `/kick`, bans, the whitelist, the idle timeout, a duplicate login, an
//! outdated client or server, illegal packets and chat spam; plus the bind failure at startup.
//!
//! `kick_dual_vanilla`  runs the scenarios against the real vanilla server and checks them against the committed
//!                      observations in `tests/fixtures/kick_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them;
//!                      `KICK_DUAL_ONLY=name` runs one scenario).
//! `kick_dual_ours`     runs them against `mc-server` and fails on any difference from the fixtures.
//!
//! Every server and client event is asserted by order (a step collects what happened after its action), never by time.
use std::cell::Cell;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::common::dual::PROPS;
use crate::common::{join, ours};
use mc_protocol::packets::handshake::{
    ClientIntent, ClientIntentionPacket, ServerboundHandshakePacket,
};
use mc_protocol::packets::login::ClientboundLoginPacket;
use mc_protocol::packets::play::{
    ClientboundPlayPacket, ServerboundClientTickEndPacket, ServerboundMovePlayerPosPacket,
    ServerboundPlayPacket,
};
use mc_testbot::Bot;
use mc_testbot::conn::Conn;
use mc_testbot::harness::repo_root;
use mc_testbot::join::BotOptions;
use mc_testkit::Server;

pub(crate) type Run = for<'a> fn(&'a Server) -> Pin<Box<dyn Future<Output = Vec<String>> + 'a>>;

pub(crate) struct Case {
    pub(crate) name: &'static str,
    pub(crate) props: &'static [&'static str],
    pub(crate) run: Run,
    /// How often the vanilla recording may be taken again when it differs from the fixture: its outcome is a race
    /// between two threads (see `stop_with_active_client`).
    pub(crate) attempts: u32,
    /// Called with the server directory before the first launch (ban lists, ops, ...).
    pub(crate) prepare: Option<fn(&std::path::Path)>,
    /// Run the scenario as soon as the process is launched instead of once the server answers status pings (what
    /// happens while the server is still starting).
    pub(crate) unready: bool,
    /// Command-line arguments after `nogui` (`--demo`, `--bonusChest`).
    pub(crate) args: &'static [&'static str],
}

macro_rules! case {
    ($name:ident, $props:expr) => {
        case!($name, $props, 1)
    };
    ($name:ident, $props:expr, $attempts:expr) => {
        case!($name, $props, $attempts, None)
    };
    ($name:ident, $props:expr, $attempts:expr, $prepare:expr) => {
        case!($name, $props, $attempts, $prepare, false)
    };
    ($name:ident, $props:expr, $attempts:expr, $prepare:expr, $unready:expr) => {
        case!($name, $props, $attempts, $prepare, $unready, &[])
    };
    ($name:ident, $props:expr, $attempts:expr, $prepare:expr, $unready:expr, $args:expr) => {
        Case {
            name: stringify!($name),
            props: $props,
            run: |s| Box::pin($name(s)),
            attempts: $attempts,
            prepare: $prepare,
            unready: $unready,
            args: $args,
        }
    };
}
pub(crate) use case;

const CASES: &[Case] = &[
    case!(stop_with_player, PROPS),
    case!(stop_with_active_client, PROPS, 3),
    case!(stop_during_sprint, PROPS),
    case!(kick_command, PROPS),
    case!(ban_commands, PROPS),
    case!(whitelist_enforced, &["enforce-whitelist=true"]),
    case!(whitelist_not_enforced, PROPS),
    case!(duplicate_login, PROPS),
    case!(outdated_client_and_server, PROPS),
    case!(illegal_packets, PROPS),
    case!(chat_spam, PROPS),
    case!(
        idle_timeout,
        &["player-idle-timeout=1", "difficulty=peaceful"]
    ),
];

fn fixture_path(scenario: &str) -> std::path::PathBuf {
    fixture_in("kick_dual", scenario)
}

fn fixture_in(dir: &str, scenario: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join(format!("{scenario}.txt"))
}

/// The scenario's own properties after the shared ones (a later entry of the same key wins).
fn props(case: &Case) -> Vec<&'static str> {
    PROPS.iter().chain(case.props).copied().collect()
}

// ---- observation helpers ---------------------------------------------------------------------------------------

/// Observation lines of one scenario: `> step` headers followed by what the step caused.
pub(crate) struct Rec<'a> {
    pub(crate) server: &'a Server,
    pub(crate) out: Vec<String>,
    log_mark: usize,
    /// Leave the `lost connection` and `left the game` lines out of the log (vanilla only logs them when the closing
    /// of the connection wins a race, see `stop_with_player`).
    pub(crate) drop_leave_lines: bool,
    /// Keep the order of the events of one step. Off: they are sorted, because vanilla's order of the leave messages
    /// against the command feedback is a race (see the module documentation).
    pub(crate) ordered: bool,
    /// Replace the position of a `moved too quickly!` line (where a position beyond the border is clamped to depends
    /// on the packets before it).
    pub(crate) mask_moves: bool,
}

/// A connected bot and how much of its packet log is already reported.
pub(crate) struct Client {
    pub(crate) name: &'static str,
    pub(crate) bot: Arc<Bot>,
    seen: Cell<usize>,
}

pub(crate) const SETTLE: Duration = Duration::from_millis(1500);

impl<'a> Rec<'a> {
    pub(crate) fn new(server: &'a Server) -> Self {
        Rec {
            server,
            out: Vec::new(),
            log_mark: server.log().lines().count(),
            ordered: false,
            mask_moves: true,
            drop_leave_lines: false,
        }
    }

    pub(crate) fn step(&mut self, text: &str) {
        self.out.push(format!("> {text}"));
    }

    /// A console command.
    pub(crate) fn console(&mut self, command: &str) {
        self.step(&format!("console {command}"));
        self.server.cmd(command);
    }

    pub(crate) async fn join(&mut self, name: &'static str) -> Client {
        self.step(&format!("join {name}"));
        let bot = join("127.0.0.1", self.server.port, name).await;
        Client {
            name,
            bot,
            seen: Cell::new(0),
        }
    }

    /// A console command that lifts a ban, then the banned player logs in again.
    async fn console_then_join(&mut self, command: &str, watcher: &Client) -> Client {
        self.console(command);
        self.settle(&[], &[watcher], Duration::ZERO).await;
        self.try_join("Victim")
            .await
            .expect("the player may log in")
    }

    /// The outcome of a login attempt: the joined client or the refusal text.
    pub(crate) async fn try_join(&mut self, name: &'static str) -> Option<Client> {
        self.step(&format!("login {name}"));
        let mut options = BotOptions::new("127.0.0.1", self.server.port, name);
        options.timeout = Duration::from_secs(60);
        let client = match Bot::connect(options).await {
            Ok(bot) => {
                let _ = bot.wait_joined(Duration::from_secs(60)).await;
                let _ = bot.wait_ready(Duration::from_secs(60)).await;
                self.out.push(format!("{name} logged in"));
                Some(Client {
                    name,
                    bot: Arc::new(bot),
                    seen: Cell::new(0),
                })
            }
            Err(e) => {
                self.out.push(format!(
                    "{name} refused: {}",
                    e.to_string().replace('\n', "\\n")
                ));
                None
            }
        };
        tokio::time::sleep(SETTLE).await;
        self.log();
        client
    }

    /// Waits until every client in `gone` is disconnected (up to `limit`), then reports for each client in `all` its
    /// disconnect reason or `connected`, the packets since the last report, and finally the server log since the last
    /// report.
    pub(crate) async fn settle(&mut self, gone: &[&Client], all: &[&Client], limit: Duration) {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline && gone.iter().any(|c| c.bot.disconnect_reason().is_none())
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(SETTLE).await;
        self.wait_quiet(all).await;
        for c in all {
            self.out.push(format!(
                "{} {}",
                c.name,
                c.bot
                    .disconnect_reason()
                    .map_or("connected".to_string(), |r| format!("disconnected: {r}"))
            ));
            let text = c.bot.log_text();
            let lines: Vec<&str> = text.lines().collect();
            let events = lines[c.seen.get().min(lines.len())..]
                .iter()
                .filter_map(|l| l.split_once("ms ").map(|(_, rest)| rest))
                // how many `/list` replies of a burst come before the kick depends on how the packets split into ticks
                .filter(|r| !r.contains("commands.list.players"))
                .filter(|r| {
                    r.starts_with("<- play/system_chat")
                        || r.starts_with("<- play/disconnect")
                        || r.starts_with("<- play/player_info_remove")
                })
                .map(|r| format!("  {}: {r}", c.name))
                .collect();
            self.push_group(events);
            c.seen.set(lines.len());
        }
        self.log();
    }

    /// Waits until neither the server log nor the packets `settle` reports grew for a moment: a loaded machine delays the
    /// leave and join events past `SETTLE`.
    async fn wait_quiet(&self, all: &[&Client]) {
        let count = || {
            let packets: usize = all
                .iter()
                .map(|c| {
                    c.bot
                        .log_text()
                        .lines()
                        .filter(|l| {
                            [
                                "<- play/system_chat",
                                "<- play/disconnect",
                                "<- play/player_info_remove",
                            ]
                            .iter()
                            .any(|k| l.split_once("ms ").is_some_and(|(_, r)| r.starts_with(k)))
                        })
                        .count()
                })
                .sum();
            packets + self.server.log().lines().count()
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last = count();
        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let now = count();
            if now == last {
                break;
            }
            last = now;
        }
    }
    /// The server log lines since the last report.
    pub(crate) fn log(&mut self) {
        let text = self.server.log();
        let lines: Vec<&str> = text.lines().collect();
        let events = lines[self.log_mark.min(lines.len())..]
            .iter()
            .filter_map(|l| log_message(l, self.mask_moves))
            .filter(|l| {
                !(self.drop_leave_lines
                    && (l.contains("lost connection") || l.contains("left the game")))
            })
            .collect();
        self.log_mark = lines.len();
        self.push_group(events);
    }

    fn push_group(&mut self, mut events: Vec<String>) {
        if !self.ordered {
            events.sort();
        }
        self.out.extend(events);
    }
}

/// The message of a log line of the server thread; every other thread, and lines whose text depends on the run
/// (entity ids, coordinates, timings), are left out.
fn log_message(line: &str, mask_moves: bool) -> Option<String> {
    let rest = line.strip_prefix('[')?.split_once("] ")?.1;
    let rest = rest.strip_prefix('[')?;
    let (thread_level, message) = rest.split_once("]: ")?;
    if !thread_level.starts_with("Server thread/") {
        return None;
    }
    const VOLATILE: &[&str] = &[
        "logged in with entity id",
        "Preparing",
        "Done (",
        "Time elapsed",
        "Saving chunks for level",
        "ThreadedAnvilChunkStorage",
        "Can't keep up",
        "Starting",
        // vanilla logs it when the inline `handleDisconnection` and the connection tick both run: a race
        "handleDisconnection() called twice",
        // how many messages of a burst are relayed before the kick depends on how the packets split into ticks
        "[Not Secure] <",
        // vanilla logs it when a player's chunk below arrives after the player: a race against the chunk delivery
        "standing on air - force-sending",
    ];
    if VOLATILE.iter().any(|v| message.contains(v)) {
        return None;
    }
    // The reason of `lost connection` is the one `disconnect` was given, unless the client closed its end first (a bot
    // does as soon as it reads the kick): then vanilla logs `Disconnected`. The client's own report has the reason.
    let message = match message.split_once("lost connection: ") {
        Some((head, _)) => format!("{head}lost connection: <reason>"),
        None => message.to_string(),
    };
    // where a position beyond the border is clamped to depends on the packets before it
    let message = match message.split_once("moved too quickly! ") {
        Some((head, _)) if mask_moves => format!("{head}moved too quickly! <position>"),
        _ => message,
    };
    // an average over the packets of the last seconds
    let message = match message.split_once("exceeded rate-limit (sent ") {
        Some((head, _)) => format!("{head}exceeded rate-limit (sent <n> packets per second)"),
        None => message,
    };
    // the game time depends on how long the server ran
    let message = match message.split_once("The game time is ") {
        Some((head, _)) => format!("{head}The game time is <n> tick(s)"),
        None => message,
    };
    Some(format!("  log: [{thread_level}] {}", mask_ports(&message)))
}

/// The translation key of a component (`outdated_client` and `incompatible` read the same in English).
fn translation_key(component: &mc_text::Component) -> String {
    let debug = format!("{component:?}");
    debug
        .split_once("key: \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or("<literal>".to_string(), |(key, _)| key.to_string())
}

/// `/127.0.0.1:51234` becomes `/127.0.0.1:<port>`.
fn mask_ports(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("/127.0.0.1:") {
        let (head, tail) = rest.split_at(at + "/127.0.0.1:".len());
        out.push_str(head);
        out.push_str("<port>");
        rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

// ---- scenarios -------------------------------------------------------------------------------------------------

/// A client that logs in and then only listens: it never answers a keep-alive and sends nothing after the join, so no
/// unread data is in flight when the server closes the connection (a close with unread data resets it and can
/// swallow the final packet). Silence also trips the server's 30 second read timeout, which closes the connection
/// without a reason: [`Silent::join_pinging`] sends a harmless packet every second to get past it.
pub(crate) struct Silent {
    reader: tokio::task::JoinHandle<(Option<String>, String)>,
}

impl Silent {
    pub(crate) async fn join(s: &Server, name: &str) -> Silent {
        Self::join_with(s, name, false).await
    }

    pub(crate) async fn join_pinging(s: &Server, name: &str) -> Silent {
        Self::join_with(s, name, true).await
    }

    async fn join_with(s: &Server, name: &str, ping: bool) -> Silent {
        let mut conn = Conn::connect(
            "127.0.0.1",
            s.port,
            Duration::from_secs(30),
            Default::default(),
        )
        .await
        .expect("connect");
        let options = BotOptions::new("127.0.0.1", s.port, name);
        mc_testbot::join::login_and_configure(&mut conn, &options)
            .await
            .expect("login");
        s.wait_log(&format!("{name} joined the game"), 30)
            .expect("joined");
        let Conn {
            mut reader,
            mut writer,
            ..
        } = conn;
        // the write half lives as long as the reader: dropping it would close the socket
        let writer = tokio::spawn(async move {
            let tick_end = mc_testbot::conn::encode(&ServerboundPlayPacket::ClientTickEnd(
                ServerboundClientTickEndPacket,
            ))
            .expect("encode");
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if ping {
                    let _ = writer.write_payload(&tick_end).await;
                    let _ = writer.flush().await;
                }
            }
        });
        let reader = tokio::spawn(async move {
            let mut reason = None;
            let end = loop {
                match reader.read_payload().await {
                    Ok(payload) => {
                        if let Ok(ClientboundPlayPacket::Disconnect(d)) =
                            ClientboundPlayPacket::decode_payload(&payload)
                        {
                            reason = Some(d.reason.get_string());
                        }
                    }
                    Err(e) => break e.to_string(),
                }
            };
            writer.abort();
            (reason, end)
        });
        Silent { reader }
    }

    pub(crate) async fn report(self, r: &mut Rec<'_>) {
        let (reason, end) = self.reader.await.expect("reader task");
        r.out.push(format!(
            "client received disconnect: {}",
            reason.as_deref().unwrap_or("<none>")
        ));
        r.out.push(format!("client stream ended: {end}"));
        r.log();
    }
}

/// `/stop` with a client that is connected but silent. `PlayerList.removeAll` kicks it with
/// `multiplayer.disconnect.server_shutdown`: the client receives the reason. The `lost connection` and `left the game`
/// lines are not compared: vanilla runs `handleDisconnection` inline from `disconnect` and logs them only when the
/// netty thread has already closed the channel, which an idle connection's thread rarely has (a busy one usually
/// has, see `stop_with_active_client`).
async fn stop_with_player(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    r.drop_leave_lines = true;
    let client = Silent::join(s, "Bot").await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    r.log();
    r.console("stop");
    s.wait_exit(Duration::from_secs(60));
    client.report(&mut r).await;
    r.out
}

/// `/stop` with a client that keeps sending (a bot ticking at 20 Hz), the usual case: the player leaves inside
/// `removeAll`, between `Saving players` and `Saving worlds` (vanilla's inline `handleDisconnection` wins the race
/// against the closing netty thread, so the recording repeats until it shows that outcome). What the client itself
/// receives is not compared: a close with unread data resets the connection and can swallow the kick.
async fn stop_with_active_client(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    let bot = r.join("Bot").await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    r.log();
    r.console("stop");
    s.wait_exit(Duration::from_secs(60));
    tokio::time::sleep(SETTLE).await;
    r.out.push(format!(
        "client disconnected: {}",
        bot.bot.disconnect_reason().is_some()
    ));
    r.log();
    r.out
}

/// `/stop` while the server is sprinting (`/tick sprint`): the same sequence as without the sprint.
async fn stop_during_sprint(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    r.drop_leave_lines = true;
    let client = Silent::join(s, "Bot").await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    r.log();
    r.out.clear();
    r.console("tick sprint 1000000");
    tokio::time::sleep(Duration::from_secs(2)).await;
    r.log();
    r.console("stop");
    s.wait_exit(Duration::from_secs(60));
    client.report(&mut r).await;
    r.out
}

async fn kick_command(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let mut victim = r.join("Victim").await;
    let watcher = r.join("Watcher").await;
    r.console("op Watcher");
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
    r.out.clear();

    r.console("kick Victim");
    r.settle(&[&victim], &[&victim, &watcher], T).await;
    victim = r.join("Victim").await;
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
    r.console("kick Victim a reason with words");
    r.settle(&[&victim], &[&victim, &watcher], T).await;
    victim = r.join("Victim").await;
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
    r.step("Watcher runs /kick Victim from an operator");
    watcher.bot.command("kick Victim kicked by op");
    r.settle(&[&victim], &[&victim, &watcher], T).await;
    victim = r.join("Victim").await;
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
    r.console("kick @a everybody out");
    r.settle(&[&victim, &watcher], &[&victim, &watcher], T)
        .await;
    r.out
}

async fn ban_commands(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let victim = r.join("Victim").await;
    let watcher = r.join("Watcher").await;
    r.console("op Watcher");
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
    r.out.clear();

    r.console("ban Victim griefing");
    r.settle(&[&victim], &[&victim, &watcher], T).await;
    r.try_join("Victim").await;
    let victim = r.console_then_join("pardon Victim", &watcher).await;
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;

    r.console("ban Victim");
    r.settle(&[&victim], &[&victim, &watcher], T).await;
    let victim = r.console_then_join("pardon Victim", &watcher).await;
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;

    r.step("Watcher runs /ban Victim from an operator");
    watcher.bot.command("ban Victim by op");
    r.settle(&[&victim], &[&victim, &watcher], T).await;
    let victim = r.console_then_join("pardon Victim", &watcher).await;
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;

    r.console("ban-ip 127.0.0.1 no more");
    r.settle(&[&victim, &watcher], &[&victim, &watcher], T)
        .await;
    r.try_join("Victim").await;
    r.console("pardon-ip 127.0.0.1");
    r.settle(&[], &[], Duration::ZERO).await;
    let victim = r.try_join("Victim").await.expect("unbanned address joins");
    r.settle(&[], &[&victim], Duration::ZERO).await;

    r.console("ban-ip Victim by name");
    r.settle(&[&victim], &[&victim], T).await;
    r.out
}

async fn whitelist_enforced(s: &Server) -> Vec<String> {
    whitelist(s, true).await
}

async fn whitelist_not_enforced(s: &Server) -> Vec<String> {
    whitelist(s, false).await
}

async fn whitelist(s: &Server, enforced: bool) -> Vec<String> {
    let mut r = Rec::new(s);
    let mut victim = r.join("Victim").await;
    let watcher = r.join("Watcher").await;
    r.console("op Watcher");
    r.console("whitelist add Watcher");
    r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
    r.out.clear();

    // `kickUnlistedPlayers` checks the list itself: an operator that is not listed goes too.
    for (command, victim_kicked) in [
        ("whitelist on", enforced),
        ("whitelist add Victim", false),
        ("whitelist remove Victim", enforced),
        ("whitelist off", false),
        ("whitelist on", enforced),
        ("whitelist add Victim", false),
    ] {
        r.console(command);
        let limit = if victim_kicked { T } else { Duration::ZERO };
        let gone: &[&Client] = if victim_kicked { &[&victim] } else { &[] };
        r.settle(gone, &[&victim, &watcher], limit).await;
        // a player that was kicked tries to come back: refused until listed (or the list is off)
        if victim.bot.disconnect_reason().is_some()
            && let Some(back) = r.try_join("Victim").await
        {
            victim = back;
            r.settle(&[], &[&victim, &watcher], Duration::ZERO).await;
        }
    }
    r.step("whitelist.json emptied on disk");
    std::fs::write(s.dir().join("whitelist.json"), "[]").expect("write whitelist.json");
    r.console("whitelist reload");
    let gone: &[&Client] = if enforced { &[&victim, &watcher] } else { &[] };
    let limit = if enforced { T } else { Duration::ZERO };
    r.settle(gone, &[&victim, &watcher], limit).await;
    r.out
}

async fn duplicate_login(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let first = r.join("Bot").await;
    let watcher = r.join("Watcher").await;
    r.settle(&[], &[&first, &watcher], Duration::ZERO).await;
    r.out.clear();
    r.step("login Bot a second time");
    let mut second = r.join("Bot").await;
    second.name = "Bot2";
    r.settle(&[&first], &[&first, &second, &watcher], T).await;
    r.out
}

async fn outdated_client_and_server(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let current = mc_protocol::PROTOCOL_VERSION;
    for (what, version) in [
        ("one version older", current - 1),
        ("one version newer", current + 1),
        ("far older", 100),
        ("below the first versioned protocol", 753),
        ("the first versioned protocol", 754),
    ] {
        r.step(what);
        let log = Default::default();
        let mut conn = Conn::connect("127.0.0.1", s.port, Duration::from_secs(20), log)
            .await
            .expect("connect");
        conn.send(&ServerboundHandshakePacket::ClientIntention(
            ClientIntentionPacket {
                protocol_version: version,
                host_name: "127.0.0.1".into(),
                port: s.port,
                intention: ClientIntent::Login,
            },
        ))
        .await
        .expect("handshake");
        let answer = match conn.recv::<ClientboundLoginPacket>().await {
            Ok(ClientboundLoginPacket::LoginDisconnect(d)) => {
                format!(
                    "login disconnect: {} ({})",
                    d.reason.0.get_string(),
                    translation_key(&d.reason.0)
                )
            }
            Ok(other) => format!("unexpected {other:?}"),
            Err(e) => format!("error: {e}"),
        };
        r.out.push(answer);
        tokio::time::sleep(SETTLE).await;
        r.log();
    }
    r.out
}

async fn illegal_packets(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.log();
    type Act = fn(&Bot);
    let acts: &[(&str, Act)] = &[
        ("position is NaN", |b| {
            b.send(ServerboundPlayPacket::MovePlayerPos(
                ServerboundMovePlayerPosPacket {
                    x: f64::NAN,
                    y: 0.0,
                    z: 0.0,
                    on_ground: false,
                    horizontal_collision: false,
                },
            ))
        }),
        ("position beyond the world", |b| {
            b.send(ServerboundPlayPacket::MovePlayerPos(
                ServerboundMovePlayerPosPacket {
                    x: 4.0e7,
                    y: 0.0,
                    z: 0.0,
                    on_ground: false,
                    horizontal_collision: false,
                },
            ))
        }),
        ("chat with a section sign", |b| b.chat("bad \u{a7}text")),
        ("command with a section sign", |b| {
            b.command("say bad \u{a7}text")
        }),
        ("attack itself", |b| {
            let id = b.with_state(|st| st.entity_id);
            b.attack(id)
        }),
        ("unexpected keep alive", |b| {
            b.send(ServerboundPlayPacket::KeepAlive(
                mc_protocol::packets::play::ServerboundKeepAlivePacket { id: 123_456 },
            ))
        }),
    ];
    for (what, act) in acts {
        let bot = r.join("Bot").await;
        r.log();
        r.step(what);
        act(&bot.bot);
        r.settle(&[&bot], &[&bot], T).await;
        if bot.bot.disconnect_reason().is_none() {
            // not kicked: the next round must not log in a second time with the same name
            bot.bot.disconnect().await;
            tokio::time::sleep(SETTLE).await;
            r.log();
        }
    }
    r.out
}

async fn chat_spam(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    for what in ["messages", "commands"] {
        let bot = r.join("Bot").await;
        r.log();
        r.step(&format!("a burst of {what}"));
        for _ in 0..60 {
            if what == "messages" {
                bot.bot.chat("hello");
            } else {
                bot.bot.command("list");
            }
        }
        r.settle(&[&bot], &[&bot], T).await;
    }
    r.out
}

async fn idle_timeout(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let bot = r.join("Bot").await;
    r.log();
    r.out.clear();
    r.step("stand still for the idle timeout");
    r.settle(&[&bot], &[&bot], Duration::from_secs(120)).await;
    r.out
}

pub(crate) const T: Duration = Duration::from_secs(30);

// ---- the tests -------------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn kick_dual_vanilla() {
    record_vanilla("kick_dual", "KICK_DUAL_ONLY", CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn kick_dual_ours() {
    compare_ours("kick_dual", "KICK_DUAL_ONLY", CASES).await;
}

/// Runs `cases` against the vanilla server and checks them against `tests/fixtures/<dir>/*.txt`
/// (`MC_UPDATE_FIXTURES=1` rewrites them; the environment variable `only_var` names one case to run).
pub(crate) async fn record_vanilla(dir: &str, only_var: &str, cases: &[Case]) {
    let only = std::env::var(only_var).ok();
    let update = std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    for case in cases
        .iter()
        .filter(|c| only.as_deref().is_none_or(|o| o == c.name))
    {
        let path = fixture_in(dir, case.name);
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        for attempt in 1..=case.attempts {
            let mut builder = mc_testbot::harness::vanilla(12345, "survival", &props(case));
            if let Some(prepare) = case.prepare {
                builder = builder.prepare(prepare);
            }
            for arg in case.args {
                builder = builder.arg(arg);
            }
            let server = if case.unready {
                builder.start_unready()
            } else {
                builder.start()
            }
            .expect("start vanilla");
            let lines = (case.run)(&server).await;
            server.stop();
            println!("SCENARIO {}:\n  {}", case.name, lines.join("\n  "));
            if update {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, lines.join("\n") + "\n").unwrap();
                break;
            }
            if expected.trim_end() == lines.join("\n") {
                break;
            }
            if attempt == case.attempts {
                failures.push(case.name);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "vanilla observations differ from the committed fixtures: {failures:?}"
    );
}

/// Runs `cases` against our server and fails on any difference from the fixtures of [`record_vanilla`]; the
/// differences are also written to `run/<dir>-ours.diff`.
pub(crate) async fn compare_ours(dir: &str, only_var: &str, cases: &[Case]) {
    let only = std::env::var(only_var).ok();
    let mut diffs = Vec::new();
    for case in cases
        .iter()
        .filter(|c| only.as_deref().is_none_or(|o| o == c.name))
    {
        let mut builder = ours(12345, "survival", &props(case));
        if let Some(prepare) = case.prepare {
            builder = builder.prepare(prepare);
        }
        for arg in case.args {
            builder = builder.arg(arg);
        }
        let server = if case.unready {
            builder.start_unready()
        } else {
            builder.start()
        }
        .expect("start our server");
        let lines: Vec<String> = (case.run)(&server)
            .await
            .iter()
            .flat_map(|l| l.split('\n').map(str::to_string).collect::<Vec<_>>())
            .collect();
        server.stop();
        let actual_dir = repo_root().join(format!("run/{}-ours", dir.replace('_', "-")));
        let _ = std::fs::create_dir_all(&actual_dir);
        let _ = std::fs::write(
            actual_dir.join(format!("{}.txt", case.name)),
            lines.join("\n") + "\n",
        );
        let expected: Vec<String> = std::fs::read_to_string(fixture_in(dir, case.name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        for i in 0..expected.len().max(lines.len()) {
            let (e, o) = (
                expected.get(i).map_or("<none>", String::as_str),
                lines.get(i).map_or("<none>", String::as_str),
            );
            if e != o {
                diffs.push(format!(
                    "{} #{i}\n  vanilla: {e}\n  ours:    {o}",
                    case.name
                ));
            }
        }
    }
    let _ = std::fs::create_dir_all(repo_root().join("run"));
    let _ = std::fs::write(
        repo_root().join(format!("run/{}-ours.diff", dir.replace('_', "-"))),
        diffs.join("\n"),
    );
    assert!(
        diffs.is_empty(),
        "our server differs from vanilla:\n{}",
        diffs.join("\n")
    );
}

// ---- bind failure ----------------------------------------------------------------------------------------------

/// The startup output of a server that cannot bind its port (somebody else holds it): the warnings, the failure and
/// the exit; timestamps, the port and stack traces are left out.
fn bind_failure(builder: mc_testkit::Builder) -> Vec<String> {
    let blocker = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = blocker.local_addr().unwrap().port().to_string();
    let server = builder
        .property("server-port", &port)
        .start_unready()
        .expect("launch");
    let exited = server.wait_exit(Duration::from_secs(60)).is_some();
    let mut lines: Vec<String> = server
        .log()
        .lines()
        .filter(|l| l.contains("] [Server thread/"))
        .filter_map(|l| {
            l.split_once("] ")
                .map(|(_, rest)| rest.replace(&port, "<port>"))
        })
        .filter(|l| !l.contains("crash report has been saved") && !l.contains("Time elapsed"))
        .collect();
    lines.push(format!("exited: {exited}"));
    lines
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn bind_failure_vanilla() {
    let lines = bind_failure(mc_testbot::harness::vanilla(12345, "survival", PROPS));
    let path = fixture_path("bind_failure");
    println!("{}", lines.join("\n"));
    if std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(expected.trim_end(), lines.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn bind_failure_ours() {
    let lines = bind_failure(ours(12345, "survival", PROPS));
    println!("{}", lines.join("\n"));
    let expected = std::fs::read_to_string(fixture_path("bind_failure")).unwrap_or_default();
    assert_eq!(expected.trim_end(), lines.join("\n"));
}
