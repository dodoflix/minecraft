//! The policies the play-state packet listener applies to a connected player, observed through the same bot script on
//! the vanilla server and on OUR server: chat and command spam thresholds (operators are exempt), floating, the
//! speed check of a movement packet, the spawn protection of a non-operator (break, place and use inside and outside
//! the radius, the operator bypass, the radius edge), a view distance change through the management API and the
//! pause of an empty server.
//!
//! `play_policy_dual_vanilla` runs the scenarios against the real vanilla server and checks them against the committed
//! observations in `tests/fixtures/play_policy_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them;
//! `PLAY_POLICY_DUAL_ONLY=name` runs one scenario). `play_policy_dual_ours` runs them against `mc-server`.
use std::path::Path;
use std::time::{Duration, Instant};

use crate::kick_dual::{Case, Rec, SETTLE, T, case, compare_ours, record_vanilla};
use crate::server_policy_dual::{operator, write};
use mc_core::direction::Direction;
use mc_core::pos::block_pos::BlockPos;
use mc_protocol::packets::play::{
    InteractionHand, ServerboundMovePlayerPosPacket, ServerboundPlayPacket,
};
use mc_testbot::Bot;
use mc_testkit::Server;

const DIR: &str = "play_policy_dual";
const ONLY: &str = "PLAY_POLICY_DUAL_ONLY";

const SECRET: &str = "abcdefghijABCDEFGHIJ0123456789abcdefghij";

const CASES: &[Case] = &[
    case!(
        chat_spam_one_second,
        &["chat-spam-threshold-seconds=1", "difficulty=peaceful"],
        1,
        Some(prepare_operator)
    ),
    case!(
        chat_spam_two_seconds,
        &["chat-spam-threshold-seconds=2", "difficulty=peaceful"],
        1,
        Some(prepare_operator)
    ),
    case!(
        command_spam_one_second,
        &["command-spam-threshold-seconds=1", "difficulty=peaceful"],
        1,
        Some(prepare_operator)
    ),
    case!(
        command_spam_two_seconds,
        &["command-spam-threshold-seconds=2", "difficulty=peaceful"],
        1,
        Some(prepare_operator)
    ),
    case!(floating, &["difficulty=peaceful"]),
    case!(moved_too_quickly, &["difficulty=peaceful"]),
    case!(
        spawn_protection,
        &["spawn-protection=16", "difficulty=peaceful"],
        1,
        Some(prepare_operator)
    ),
    case!(
        view_distance_rpc,
        &[
            "management-server-enabled=true",
            "management-server-host=127.0.0.1",
            "management-server-tls-enabled=false",
            "management-server-secret=abcdefghijABCDEFGHIJ0123456789abcdefghij",
            "difficulty=peaceful",
        ]
    ),
    case!(pause_when_empty, &["pause-when-empty-seconds=1"]),
];

fn prepare_operator(dir: &Path) {
    write(dir, "ops.json", &[operator("Operator", false)]);
}

/// What `bot` received since line `mark` of its packet log, of the given kinds, sorted. A `block_update` counts only
/// for the block column `column` (random ticks elsewhere change blocks too).
fn heard(bot: &Bot, mark: usize, kinds: &[&str], column: Option<(i32, i32)>) -> Vec<String> {
    let text = bot.log_text();
    let mut lines: Vec<String> = text
        .lines()
        .skip(mark)
        .filter_map(|l| l.split_once("ms ").map(|(_, rest)| rest))
        .filter(|r| kinds.iter().any(|k| r.starts_with(k)))
        .filter(|r| {
            let Some((x, z)) = column.filter(|_| r.starts_with("<- play/block_update")) else {
                return true;
            };
            r.contains(&format!("x={x}, ")) && r.contains(&format!("z={z}}}"))
        })
        .map(|r| format!("  {r}"))
        .collect();
    lines.sort();
    lines
}

fn mark(bot: &Bot) -> usize {
    bot.log_text().lines().count()
}

/// Chat messages or `/list` commands a player sends, spaced and in bursts, against a threshold of `seconds`: the
/// throttler counts 20 per message, loses one per tick and kicks at `20 * seconds`, so with one second a message is
/// over the limit, with two a burst of three is and two messages a second and a half apart are not. Operators are
/// exempt. A burst after the kick would race the closing connection, so the one-second case has none.
async fn spam(s: &Server, command: bool, seconds: u32) -> Vec<String> {
    let mut r = Rec::new(s);
    let send = |bot: &Bot| {
        if command {
            bot.command("list")
        } else {
            bot.chat("hello")
        }
    };
    let mut rounds = vec![
        ("Bot", "one", 1, Duration::ZERO),
        ("Bot", "two a second and a half apart", 2, SETTLE),
    ];
    if seconds > 1 {
        rounds.push(("Bot", "a burst of three", 3, Duration::ZERO));
    }
    rounds.push(("Operator", "a burst of thirty", 30, Duration::ZERO));
    for (name, what, count, spacing) in rounds {
        let client = r.join(name).await;
        r.log();
        r.step(&format!("{name}: {what}"));
        for i in 0..count {
            if i > 0 {
                tokio::time::sleep(spacing).await;
            }
            send(&client.bot);
        }
        r.settle(&[&client], &[&client], Duration::from_secs(5))
            .await;
        if client.bot.disconnect_reason().is_none() {
            client.bot.disconnect().await;
            tokio::time::sleep(SETTLE).await;
            r.log();
        }
    }
    r.out
}

async fn chat_spam_one_second(s: &Server) -> Vec<String> {
    spam(s, false, 1).await
}

async fn chat_spam_two_seconds(s: &Server) -> Vec<String> {
    spam(s, false, 2).await
}

async fn command_spam_one_second(s: &Server) -> Vec<String> {
    spam(s, true, 1).await
}

async fn command_spam_two_seconds(s: &Server) -> Vec<String> {
    spam(s, true, 2).await
}

/// The client believes it flies (its abilities allow it) once it is high up, the server does not: every move packet
/// keeps the height, and after 80 ticks `multiplayer.disconnect.flying`. (A client on the ground drops the flag at its
/// next tick, so the flag is set after the teleport.)
async fn floating(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let bot = r.join("Bot").await;
    r.console("tp Bot 0.5 100 0.5");
    let deadline = Instant::now() + T;
    while bot.bot.position().y < 90.0 {
        assert!(Instant::now() < deadline, "the teleport never arrived");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    bot.bot.with_state_mut(|st| {
        st.body.abilities.may_fly = true;
        st.body.abilities.flying = true;
    });
    r.log();
    r.out.clear();
    let start = Instant::now();
    while bot.bot.disconnect_reason().is_none() && start.elapsed() < T {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let waited = start.elapsed();
    r.step("hover without permission");
    r.settle(&[&bot], &[&bot], Duration::ZERO).await;
    r.out.push(format!(
        "kicked after about four seconds: {}",
        (Duration::from_millis(3500)..Duration::from_millis(5500)).contains(&waited)
    ));
    r.out
}

/// A move packet that is too far away from the last position: `moved too quickly!` above ten blocks (the squared
/// distance is over 100); the player is put back with a `player_position`. Nine blocks pass without a word.
async fn moved_too_quickly(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.mask_moves = false;
    let bot = r.join("Bot").await;
    for distance in [500.0, 11.0, 9.0] {
        r.console("tp Bot 0.5 -60 0.5");
        tokio::time::sleep(SETTLE).await;
        r.log();
        let since = mark(&bot.bot);
        r.step(&format!("one packet {distance} blocks away"));
        bot.bot.send(ServerboundPlayPacket::MovePlayerPos(
            ServerboundMovePlayerPosPacket {
                x: 0.5 + distance,
                y: -60.0,
                z: 0.5,
                on_ground: true,
                horizontal_collision: false,
            },
        ));
        tokio::time::sleep(SETTLE).await;
        r.out.extend(heard(
            &bot.bot,
            since,
            &["<- play/player_position", "<- play/disconnect"],
            None,
        ));
        r.log();
    }
    r.out
}

/// Breaking, placing and using on blocks in and beyond the 16 block spawn protection of a non-operator, at its edge on
/// both axes, and the operator's own blocks on the other side of the spawn point. What the player hears and what the
/// block ends up as (asked of the server) is compared.
async fn spawn_protection(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    let player = r.join("Player").await;
    let operator = r.join("Operator").await;
    r.console("setworldspawn 0 -60 0");
    r.console("gamerule random_tick_speed 0");
    for who in ["Player", "Operator"] {
        r.console(&format!("give {who} minecraft:cobblestone 64"));
        r.console(&format!("give {who} minecraft:iron_hoe"));
    }
    tokio::time::sleep(SETTLE).await;
    r.log();
    r.out.clear();
    // (action, x, z) of the clicked block, mirrored for the operator
    let clicks = [
        ("break", 3, 0),
        ("place", 3, 2),
        ("use hoe", 3, 4),
        ("break", 16, 0),
        ("break", 17, 0),
        ("break", 0, 16),
        ("break", 0, 17),
        ("place", 16, 6),
        ("place", 17, 8),
        ("use hoe", 16, 10),
        ("use hoe", 17, 12),
        ("break", 40, 0),
        ("place", 40, 2),
        ("use hoe", 40, 4),
    ];
    for (who, client, sign) in [("Player", &player, 1), ("Operator", &operator, -1)] {
        for (action, x, z) in clicks {
            let (x, z) = (x * sign, z * sign);
            r.console(&format!(
                "tp {who} {} -60 {}",
                f64::from(x) + 0.5,
                f64::from(z) - 1.5
            ));
            tokio::time::sleep(SETTLE).await;
            r.log();
            r.step(&format!("{who}: {action} at {x} {z}"));
            let bot = &client.bot;
            let since = mark(bot);
            let pos = BlockPos::new(x, -61, z);
            bot.select_hotbar(if action == "use hoe" { 1 } else { 0 });
            if action == "break" {
                bot.start_digging(pos, Direction::Up);
                tokio::time::sleep(Duration::from_secs(3)).await;
            } else {
                let cursor = (0.5, 1.0, 0.5);
                bot.use_item_on(InteractionHand::MainHand, pos, Direction::Up, cursor, false);
                tokio::time::sleep(SETTLE).await;
            }
            r.out.extend(heard(
                bot,
                since,
                &[
                    "<- play/block_update",
                    "<- play/block_changed_ack",
                    "<- play/system_chat",
                ],
                Some((x, z)),
            ));
            let truth = match action {
                "break" => format!("execute if block {x} -61 {z} minecraft:air"),
                "place" => format!("execute if block {x} -60 {z} minecraft:cobblestone"),
                _ => format!("execute if block {x} -61 {z} minecraft:farmland"),
            };
            r.console(&truth);
            tokio::time::sleep(Duration::from_millis(500)).await;
            r.log();
        }
    }
    r.out
}

/// Waits until the bot has received no chunk or forget packet for a second: the rate the server sends them at depends on
/// the load of the machine.
async fn chunks_settled(bot: &Bot) {
    let count = || {
        bot.log_text()
            .lines()
            .filter(|l| {
                l.contains("<- play/level_chunk_with_light")
                    || l.contains("<- play/forget_level_chunk")
            })
            .count()
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    let (mut last, mut quiet_since) = (count(), Instant::now());
    while quiet_since.elapsed() < Duration::from_secs(1) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let now = count();
        if now != last {
            (last, quiet_since) = (now, Instant::now());
        }
    }
}

/// The management API call `serversettings/view_distance/set`: the reply, what the connected player is told, the
/// value afterwards and the line of `server.properties`. A value below the lowest the chunk map accepts is told to the
/// player as it was given.
async fn view_distance_rpc(s: &Server) -> Vec<String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut r = Rec::new(s);
    let bot = r.join("Bot").await;
    r.log();
    // the chunks of the join are still arriving
    chunks_settled(&bot.bot).await;
    let line = s
        .wait_log("Json-RPC Management connection listening on", 30)
        .expect("management server");
    let port = line.rsplit(':').next().unwrap().trim();
    let mut request = format!("ws://127.0.0.1:{port}/")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {SECRET}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("management connection");
    let mut since = mark(&bot.bot);
    for (id, call) in [
        r#""method":"minecraft:serversettings/view_distance""#,
        r#""method":"minecraft:serversettings/view_distance/set","params":[6]"#,
        r#""method":"minecraft:serversettings/view_distance""#,
        r#""method":"minecraft:serversettings/view_distance/set","params":[2]"#,
        r#""method":"minecraft:serversettings/view_distance/set","params":[1]"#,
    ]
    .into_iter()
    .enumerate()
    {
        r.step(call);
        ws.send(Message::text(format!(
            r#"{{"jsonrpc":"2.0","id":{id},{call}}}"#
        )))
        .await
        .unwrap();
        while let Ok(Some(Ok(Message::Text(reply)))) = tokio::time::timeout(SETTLE, ws.next()).await
        {
            r.out.push(format!("  rpc: {reply}"));
        }
        let properties = std::fs::read_to_string(s.dir().join("server.properties")).unwrap();
        r.out.extend(
            properties
                .lines()
                .filter(|l| l.starts_with("view-distance="))
                .map(|l| format!("  properties: {l}")),
        );
        chunks_settled(&bot.bot).await;
        let text = bot.bot.log_text();
        let heard: Vec<&str> = text
            .lines()
            .skip(since)
            .filter_map(|l| l.split_once("ms ").map(|(_, rest)| rest))
            .collect();
        let count = |kind: &str| heard.iter().filter(|h| h.starts_with(kind)).count();
        r.out.extend(
            heard
                .iter()
                .filter(|h| h.starts_with("<- play/set_chunk_cache_radius"))
                .map(|h| format!("  {h}")),
        );
        r.out.push(format!(
            "  chunks forgotten: {}, chunks sent: {}",
            count("<- play/forget_level_chunk"),
            count("<- play/level_chunk_with_light")
        ));
        since = text.lines().count();
    }
    r.log();
    r.out
}

/// The game time the server answers within `wait`.
async fn game_time(s: &Server, wait: Duration) -> Option<u64> {
    let since = s.log().lines().count();
    s.cmd("time query gametime");
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        let log = s.log();
        if let Some(n) = log.lines().skip(since).find_map(|l| {
            l.split_once("The game time is ")
                .and_then(|(_, n)| n.split_once(" tick"))
                .map(|(n, _)| n.to_string())
        }) {
            return Some(n.parse().expect("a game time"));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// With `pause-when-empty-seconds=1` the server stops ticking one second after its last player left (and logs it);
/// the console is still read while it is paused, a player joining resumes the game time.
async fn pause_when_empty(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    let bot = r.join("Bot").await;
    r.log();
    r.out.clear();
    bot.bot.disconnect().await;
    r.step("the last player left");
    tokio::time::sleep(Duration::from_secs(3)).await;
    r.log();
    r.step("a console command while paused");
    let first = game_time(s, Duration::from_secs(2)).await;
    r.out.push(format!("  answered: {}", first.is_some()));
    // a running server ticks a few dozen times meanwhile
    tokio::time::sleep(Duration::from_secs(2)).await;
    let second = game_time(s, Duration::from_secs(2)).await;
    r.out.push(format!(
        "  game time stands still: {}",
        first.is_some() && first == second
    ));
    r.log();
    let bot = r.join("Bot").await;
    tokio::time::sleep(SETTLE).await;
    r.log();
    r.step("game time with a player");
    let before = game_time(s, T).await.expect("an answer");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after = game_time(s, T).await.expect("an answer");
    r.out.push(format!("  advanced: {}", after > before));
    r.log();
    r.step("the player leaves");
    bot.bot.disconnect().await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    r.log();
    r.out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn play_policy_dual_vanilla() {
    record_vanilla(DIR, ONLY, CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn play_policy_dual_ours() {
    compare_ours(DIR, ONLY, CASES).await;
}
