//! `server.properties` keys with an observable effect, observed on the real vanilla server and on `mc-server` with the
//! same files: the startup log with its lines in order (`Starting minecraft server version`, `Loading properties`,
//! `Default game type`, `Preparing level`, `Done`, with the elapsed time and the ports masked), the tracking distance
//! scaled by `entity-broadcast-range-percentage`, the cap `max-chained-neighbor-updates`, the operator copies of console
//! feedback (`broadcast-console-to-ops`), the legacy user lists the converter writes before the player list reads them
//! and `bug-report-link` as a server link. Every test starts vanilla first and then our server and compares what both
//! showed.
use crate::common::join;
use crate::network_vanilla_diff::registry_transcript;
use mc_testbot::join::BotOptions;
use mc_testbot::{Bot, harness};
use mc_testkit::Server;
use std::path::Path;
use std::time::Duration;

/// `[12:00:00] [Server thread/INFO]: text` as `Server thread/INFO: text`; lines of other threads and the launcher are
/// `None`.
pub(crate) fn message(line: &str) -> Option<String> {
    let rest = line.strip_prefix('[')?.split_once("] ")?.1;
    let rest = rest.strip_prefix('[')?;
    let (thread_level, text) = rest.split_once("]: ")?;
    Some(format!("{thread_level}: {text}"))
}

/// `Starting Minecraft server on 127.0.0.1:41234`, `RCON running on 0.0.0.0:25575`: the port is not comparable.
pub(crate) fn mask_ports(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == ':' {
            let mut digits = String::new();
            while let Some(d) = chars.next_if(char::is_ascii_digit) {
                digits.push(d);
            }
            out.push_str(if digits.len() >= 4 { "<port>" } else { &digits });
        }
    }
    out
}

/// The startup log up to and including `Done`, one normalized line each: no timestamps, durations and ports masked.
/// Left out: the launcher's own output, the `Environment:` line (it depends on the machine) and the other lines of the
/// worker threads of vanilla's start. `No existing world data` and `Loaded N advancements` stay, with the number of
/// their `Worker-Main-N` thread masked: both servers log them from a worker.
fn startup(server: &Server) -> Vec<String> {
    let mut lines = Vec::new();
    for line in server.log().lines() {
        let Some(message) = message(line) else {
            continue;
        };
        let message = match message.strip_prefix("Worker-Main-") {
            Some(rest) => match rest.split_once('/') {
                Some((_, text))
                    if ["No existing world data", "advancements"]
                        .iter()
                        .any(|kept| text.contains(kept)) =>
                {
                    format!("Worker-Main-N/{text}")
                }
                _ => continue,
            },
            None => message,
        };
        if message.starts_with("ServerMain/") && message.contains("Environment:") {
            continue;
        }
        if message.contains("Can't keep up") || message.contains("Preparing spawn area") {
            continue;
        }
        let message = mask_ports(&message);
        let message = match message.split_once("Done (") {
            Some((head, _)) => format!("{head}Done (<time>)! For help, type \"help\""),
            None => message,
        };
        let message = match message.split_once("Time elapsed: ") {
            Some((head, _)) => format!("{head}Time elapsed: <time> ms"),
            None => message,
        };
        let done = message.contains("Done (");
        lines.push(message);
        if done {
            break;
        }
    }
    lines
}

/// The same scenario on the real vanilla server and on `mc-server`; `props` are `key=value` entries for both.
pub(crate) async fn on_both<F: AsyncFn(&Server, bool) -> Vec<String>>(
    props: &[&str],
    prepare: fn(&Path),
    scenario: F,
) {
    let vanilla = harness::vanilla(7, "survival", props)
        .mojang(super::common::mojang())
        .prepare(prepare)
        .start()
        .expect("start vanilla");
    let theirs = scenario(&vanilla, true).await;
    vanilla.stop();
    drop(vanilla);
    let ours = super::common::ours(7, "survival", props)
        .mojang(super::common::mojang())
        .prepare(prepare)
        .start()
        .expect("start our server");
    let ours = scenario(&ours, false).await;
    println!("== {props:?}\n{}", theirs.join("\n"));
    assert!(!theirs.is_empty(), "the scenario observed nothing");
    for (i, (a, b)) in theirs.iter().zip(&ours).enumerate() {
        assert_eq!(a, b, "line {i}: vanilla (left) vs ours (right)");
    }
    assert_eq!(theirs.len(), ours.len(), "number of lines");
}

pub(crate) fn nothing(_: &Path) {}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn startup_log_equals_vanilla() {
    let rcon = mc_testkit::free_port().to_string();
    let query = mc_testkit::free_port().to_string();
    let rcon_port = format!("rcon.port={rcon}");
    let query_port = format!("query.port={query}");
    let cases: [&[&str]; 5] = [
        &["gamemode=creative", "level-name=Another World"],
        &[
            "enable-rcon=true",
            "rcon.password=secret",
            &rcon_port,
            "enable-query=true",
            &query_port,
        ],
        &["bug-report-link=not a link"],
        &["bug-report-link=ftp://example.com/bug"],
        &[
            "bug-report-link=https://example.com/bug",
            "max-tick-time=-1",
        ],
    ];
    for props in cases {
        println!("== {props:?}");
        let listeners = props.contains(&"enable-rcon=true");
        on_both(props, nothing, async |server, _| {
            let mut lines = startup(server);
            if listeners {
                lines.extend(listener_lines(server));
            }
            lines
        })
        .await;
    }
}

/// The lines the query and RCON listeners print once the world is loaded (their threads race: sorted).
fn listener_lines(server: &Server) -> Vec<String> {
    for what in ["Query running on", "RCON running on"] {
        server.wait_log(what, 30).expect(what);
    }
    let mut lines: Vec<String> = server
        .log()
        .lines()
        .filter_map(message)
        .filter(|m| {
            [
                "GS4 status listener",
                "remote control listener",
                "running on",
            ]
            .iter()
            .any(|what| m.contains(what))
        })
        .map(|m| mask_ports(&m))
        .collect();
    lines.sort();
    lines
}

// ---- entity-broadcast-range-percentage ---------------------------------------------------------------------------

/// What a bot standing still sees of entities summoned at fixed offsets: `type dx dz` for each one the server tracks
/// for it, with the bot asking for `view_distance` chunks (it caps the tracking range). Items track within 6 chunks,
/// armor stands within 10, both scaled by the percentage.
async fn tracked_entities(
    server: &Server,
    view_distance: i8,
    offsets: &[(&str, i32)],
) -> Vec<String> {
    let mut options = BotOptions::new("127.0.0.1", server.port, "Bot");
    options.client_information.view_distance = view_distance;
    options.timeout = Duration::from_secs(120);
    let bot = Bot::connect(options).await.expect("connect");
    bot.wait_joined(Duration::from_secs(60))
        .await
        .expect("login packet");
    bot.wait_ready(Duration::from_secs(90))
        .await
        .expect("player loaded");
    let p = bot.position();
    // the entities must be summoned into chunks the bot already holds, or the tracker never sees them move in range
    let view = i32::from(view_distance) * 16;
    let farthest = offsets
        .iter()
        .map(|o| o.1)
        .filter(|dx| *dx < view)
        .max()
        .unwrap_or(0);
    let (far_x, far_z) = ((p.x as i32).saturating_add(farthest), p.z as i32);
    bot.wait_for(Duration::from_secs(90), |s| {
        s.world.has_chunk_at_block(far_x, far_z).then_some(())
    })
    .await
    .expect("the farthest chunk");
    for (kind, dx) in offsets {
        let (x, y, z) = (p.x + f64::from(*dx), p.y, p.z);
        let command = match *kind {
            "item" => format!(
                "summon item {x} {y} {z} {{Item:{{id:\"minecraft:stone\",count:1}},PickupDelay:32767s,Age:-32768s,NoGravity:1b}}"
            ),
            _ => format!("summon {kind} {x} {y} {z} {{NoGravity:1b}}"),
        };
        server.cmd(&command);
    }
    bot.wait_ticks(100).await.expect("ticking");
    let own = bot.with_state(|s| s.entity_id);
    let mut seen: Vec<String> = bot.with_state(|s| {
        s.entities
            .values()
            .filter(|e| e.id != own)
            .map(|e| {
                format!(
                    "{} {:+} {:+}",
                    e.entity_type.name(),
                    (e.pos.x - p.x).round(),
                    (e.pos.z - p.z).round()
                )
            })
            .collect()
    });
    seen.sort();
    bot.disconnect().await;
    seen
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn entity_broadcast_range_percentage_equals_vanilla() {
    let flat = "level-type=minecraft:flat";
    // item: 96 * 25% = 24 blocks, armor stand: 160 * 25% = 40 blocks
    let offsets = [
        ("item", 20),
        ("item", 30),
        ("armor_stand", 30),
        ("armor_stand", 50),
    ];
    on_both(
        &[
            flat,
            "view-distance=10",
            "entity-broadcast-range-percentage=25",
        ],
        nothing,
        async |server, _| tracked_entities(server, 6, &offsets).await,
    )
    .await;
}

/// At 200 % the range is capped by what the bot asked for: with 12 chunks everything inside the view is tracked, with
/// 8 chunks (128 blocks) only what lies within them, whatever the server's own view distance.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn entity_broadcast_range_percentage_200_equals_vanilla() {
    let props = [
        "level-type=minecraft:flat",
        "view-distance=12",
        "entity-broadcast-range-percentage=200",
    ];
    let offsets = [
        ("item", 100),
        ("item", 185),
        ("armor_stand", 150),
        ("armor_stand", 185),
    ];
    on_both(&props, nothing, async |server, _| {
        tracked_entities(server, 12, &offsets).await
    })
    .await;
    let offsets = [
        ("item", 100),
        ("item", 140),
        ("armor_stand", 120),
        ("armor_stand", 150),
    ];
    on_both(&props, nothing, async |server, _| {
        tracked_entities(server, 8, &offsets).await
    })
    .await;
}

// ---- console feedback ------------------------------------------------------------------------------------------

/// Runs console commands one by one; what each printed to the log is its output (a `say` after it marks the end).
struct Console<'a> {
    server: &'a Server,
    next: usize,
    from: usize,
}

impl<'a> Console<'a> {
    fn new(server: &'a Server) -> Self {
        let from = server.log().lines().count();
        Console {
            server,
            next: 0,
            from,
        }
    }

    /// The `Server thread` lines `command` caused.
    fn run(&mut self, command: &str) -> Vec<String> {
        self.next += 1;
        let marker = format!("console-marker-{}", self.next);
        self.server.cmd(command);
        self.server.cmd(&format!("say {marker}"));
        self.server
            .wait_log(&format!("\\[Server\\] {marker}"), 60)
            .expect("the marker reaches the log");
        let text = self.server.log();
        let lines: Vec<&str> = text.lines().collect();
        let out = lines[self.from.min(lines.len())..]
            .iter()
            .filter_map(|l| message(l))
            .filter(|l| l.starts_with("Server thread/") && !l.contains(&marker))
            .collect();
        self.from = lines.len();
        out
    }
}

/// An operator's view of console feedback: what the bot is told after the console runs commands that report to
/// operators (`sendSuccess(.., true)`), and what the console itself logs.
async fn operator_feedback(server: &Server) -> Vec<String> {
    let bot = join("127.0.0.1", server.port, "Bot").await;
    let mut console = Console::new(server);
    let mut out = Vec::new();
    out.push(format!("op: {:?}", console.run("op Bot")));
    bot.wait_ticks(20).await.expect("ticking");
    let before = bot.with_state(|s| s.chat.len());
    for command in ["time set 1000", "difficulty hard", "gamemode creative Bot"] {
        out.push(format!("{command}: {:?}", console.run(command)));
    }
    bot.wait_ticks(20).await.expect("ticking");
    let told: Vec<String> = bot.with_state(|s| {
        s.chat[before..]
            .iter()
            .map(|c| format!("{} {}", c.kind, c.text))
            .collect()
    });
    out.push(format!("the operator was told: {told:?}"));
    bot.disconnect().await;
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn broadcast_console_to_ops_equals_vanilla() {
    for value in ["true", "false"] {
        let option = format!("broadcast-console-to-ops={value}");
        let props = ["level-type=minecraft:flat", option.as_str()];
        on_both(&props, nothing, async |server, _| {
            operator_feedback(server).await
        })
        .await;
    }
}

// ---- max-chained-neighbor-updates ------------------------------------------------------------------------------

/// A line of redstone dust powered at one end: the power travels along the line through chained neighbor updates, which
/// the cap cuts short. The console's report of each step (the error the cap logs) and the power along the line.
fn chained_updates(server: &Server) -> Vec<String> {
    let mut console = Console::new(server);
    // the world spawn lies elsewhere: keep the chunks of the line loaded and wait until they are
    console.run("forceload add 0 0 31 0");
    for _ in 0..100 {
        let loaded = console.run("execute if loaded 31 -60 0");
        if loaded.iter().any(|l| l.ends_with("Test passed")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let mut out = vec![format!(
        "fill: {:?}",
        console.run("fill 0 -60 0 29 -60 0 minecraft:redstone_wire")
    )];
    out.push(format!(
        "power: {:?}",
        console.run("setblock 30 -60 0 minecraft:redstone_block")
    ));
    for x in (0..30).rev() {
        let power = (15 - (29 - x)).max(0);
        let command = format!("execute if block {x} -60 0 minecraft:redstone_wire[power={power}]");
        out.push(format!("x={x} power={power}: {:?}", console.run(&command)));
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn max_chained_neighbor_updates_equals_vanilla() {
    for cap in ["8", "1000000"] {
        let option = format!("max-chained-neighbor-updates={cap}");
        let props = ["level-type=minecraft:flat", option.as_str()];
        on_both(&props, nothing, async |server, _| chained_updates(server)).await;
    }
}

// ---- legacy user lists -----------------------------------------------------------------------------------------

const LEGACY_DATE: &str = "2020-01-02 03:04:05 +0000";

/// The pre-1.7.6 list files: names only for the operators and the whitelist, `name|created|source|expires|reason` for
/// the bans.
fn legacy_lists(dir: &Path) {
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect(name);
    write("ops.txt", "# operators\nAlice\nBob\n");
    write("white-list.txt", "Carol\nDave\n");
    write(
        "banned-players.txt",
        &format!("# bans\nErin|{LEGACY_DATE}|Admin|Forever|griefing\n"),
    );
    write(
        "banned-ips.txt",
        &format!("10.1.2.3|{LEGACY_DATE}|Admin|Forever|spam\n"),
    );
}

/// What the first start made of the legacy files: the lists as the console reads them, the files written and the old
/// files that were set aside.
fn converted_lists(server: &Server) -> Vec<String> {
    let mut out = startup(server);
    let mut console = Console::new(server);
    for command in ["whitelist list", "banlist players", "banlist ips"] {
        out.push(format!("{command}: {:?}", console.run(command)));
    }
    for file in [
        "ops.json",
        "whitelist.json",
        "banned-players.json",
        "banned-ips.json",
    ] {
        let text = std::fs::read_to_string(server.dir().join(file)).unwrap_or_default();
        out.push(format!(
            "{file}: {}",
            text.split_whitespace().collect::<String>()
        ));
    }
    for file in [
        "ops.txt",
        "white-list.txt",
        "banned-players.txt",
        "banned-ips.txt",
    ] {
        out.push(format!(
            "{file} exists: {}, {file}.converted exists: {}",
            server.dir().join(file).exists(),
            server.dir().join(format!("{file}.converted")).exists()
        ));
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn legacy_user_lists_are_converted_at_the_first_start() {
    on_both(&[], legacy_lists, async |server, _| converted_lists(server)).await;
}

// ---- bug-report-link ---------------------------------------------------------------------------------------------

/// The configuration packets a login receives before the tags (`minecraft:server_links` among them, hex) and the
/// warnings about the link the server logged.
async fn server_links(server: &Server) -> Vec<String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], server.port));
    let mut out: Vec<String> = registry_transcript(addr)
        .await
        .into_iter()
        .filter(|l| {
            !l.starts_with("minecraft:registry_data") && !l.starts_with("minecraft:update_tags")
        })
        .collect();
    out.extend(
        server
            .log()
            .lines()
            .filter(|l| l.contains("bug link"))
            .filter_map(message),
    );
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn bug_report_link_equals_vanilla() {
    for link in [
        "https://example.com/bug",
        "HTTP://Example.com/a%20b?x=1#frag",
        "not a link",
        "ftp://example.com/bug",
        "https://exa mple.com/bug",
        "",
    ] {
        let option = format!("bug-report-link={link}");
        println!("== {option}");
        on_both(
            &["level-type=minecraft:flat", option.as_str()],
            nothing,
            async |server, _| server_links(server).await,
        )
        .await;
    }
}
