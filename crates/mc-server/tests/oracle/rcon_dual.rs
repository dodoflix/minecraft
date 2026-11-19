//! RCON, observed through the same Source-RCON client on the vanilla server and on OUR server: authentication
//! (failures, repeats), commands and their feedback (a failing command, an unknown command, an answer of more than
//! 4096 characters that is sent in several packets, a surrogate pair cut at the packet boundary), requests of an unknown
//! type, packets the server drops the connection for, the position and level of the command source after the world
//! spawn moved to another dimension, the operator copies of RCON feedback (`broadcast-rcon-to-ops`) and the files the
//! server writes (`usercache.json`, the user lists).
//!
//! `rcon_dual_vanilla` runs the scenarios against the real vanilla server and checks them against the committed
//! observations in `tests/fixtures/rcon_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them; `RCON_DUAL_ONLY=name` runs
//! one scenario). `rcon_dual_ours` runs them against `mc-server`.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::kick_dual::{Case, Rec, SETTLE, case, compare_ours, record_vanilla};
use mc_testkit::Server;

const DIR: &str = "rcon_dual";
const ONLY: &str = "RCON_DUAL_ONLY";

const PASSWORD: &str = "secret";

const RCON: &[&str] = &[
    "enable-rcon=true",
    "rcon.password=secret",
    "difficulty=peaceful",
];

const CASES: &[Case] = &[
    case!(protocol, RCON),
    case!(source, &[RCON[0], RCON[1], "allow-nether=true"]),
    case!(
        broadcast_true,
        &[RCON[0], RCON[1], "broadcast-rcon-to-ops=true"]
    ),
    case!(
        broadcast_false,
        &[RCON[0], RCON[1], "broadcast-rcon-to-ops=false"]
    ),
    case!(files, RCON),
];

const AUTH: i32 = 3;
const COMMAND: i32 = 2;

/// One packet of the answer: request id, type, text.
struct Packet {
    id: i32,
    kind: i32,
    text: String,
    raw: Vec<u8>,
}

struct Rcon {
    stream: TcpStream,
    pending: Vec<u8>,
}

impl Rcon {
    fn connect(server: &Server) -> Rcon {
        server
            .wait_log("RCON running on", 120)
            .expect("the RCON listener runs");
        let stream = TcpStream::connect(("127.0.0.1", server.rcon_port.expect("an RCON port")))
            .expect("connect to RCON");
        Rcon {
            stream,
            pending: Vec::new(),
        }
    }

    fn frame(id: i32, kind: i32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend((body.len() as i32 + 10).to_le_bytes());
        out.extend(id.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(body);
        out.extend([0, 0]);
        out
    }

    fn write(&mut self, bytes: &[u8]) {
        let _ = self.stream.write_all(bytes);
    }

    /// The packets that arrive until the server is quiet (`first` for the first one), and whether it closed the
    /// connection.
    fn read(&mut self, first: Duration) -> (Vec<Packet>, bool) {
        let mut packets = Vec::new();
        let mut closed = false;
        let mut wait = first;
        loop {
            self.stream.set_read_timeout(Some(wait)).unwrap();
            let mut buf = [0u8; 8192];
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(n) => self.pending.extend(&buf[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(_) => {
                    closed = true;
                    break;
                }
            }
            wait = Duration::from_millis(400);
            while self.pending.len() >= 4 {
                let len = i32::from_le_bytes(self.pending[..4].try_into().unwrap()) as usize;
                if self.pending.len() < 4 + len {
                    break;
                }
                let body: Vec<u8> = self.pending.drain(..4 + len).skip(4).collect();
                packets.push(Packet {
                    id: i32::from_le_bytes(body[..4].try_into().unwrap()),
                    kind: i32::from_le_bytes(body[4..8].try_into().unwrap()),
                    text: String::from_utf8_lossy(&body[8..body.len().saturating_sub(2)])
                        .into_owned(),
                    raw: body[8..body.len().saturating_sub(2)].to_vec(),
                });
            }
        }
        (packets, closed)
    }

    /// Sends one packet and reports what came back.
    fn request(&mut self, r: &mut Rec<'_>, id: i32, kind: i32, body: &str) -> Vec<Packet> {
        r.step(&format!("type {kind} id {id}: {}", abbreviate(body)));
        let frame = Self::frame(id, kind, body.as_bytes());
        self.write(&frame);
        self.report(r)
    }

    fn report(&mut self, r: &mut Rec<'_>) -> Vec<Packet> {
        let (packets, closed) = self.read(Duration::from_secs(10));
        for p in &packets {
            r.out.push(format!(
                "  reply id {} type {} len {}: {}",
                p.id,
                p.kind,
                p.text.encode_utf16().count(),
                abbreviate(&p.text)
            ));
        }
        if packets.is_empty() {
            r.out.push("  no reply".to_owned());
        }
        if closed {
            r.out.push("  connection closed".to_owned());
        }
        r.log();
        packets
    }

    fn login(&mut self, r: &mut Rec<'_>) {
        self.request(r, 1, AUTH, PASSWORD);
    }
}

/// Short text as it is, long text as its ends and a checksum.
fn abbreviate(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 160 {
        return format!("{text:?}");
    }
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!(
        "{:?} ... {:?} (sum {:016x})",
        chars[..60].iter().collect::<String>(),
        chars[chars.len() - 60..].iter().collect::<String>(),
        hasher.finish()
    )
}

async fn protocol(s: &Server) -> Vec<String> {
    let mut rcon = Rcon::connect(s);
    let mut r = Rec::new(s);
    r.log();
    rcon.request(&mut r, 5, COMMAND, "list");
    rcon.request(&mut r, 6, AUTH, "wrong");
    rcon.request(&mut r, 7, AUTH, "");
    rcon.request(&mut r, 8, COMMAND, "list");
    rcon.request(&mut r, 9, AUTH, "Secret");
    rcon.request(&mut r, 10, AUTH, &format!("{PASSWORD} "));
    rcon.request(&mut r, 11, AUTH, PASSWORD);
    rcon.request(&mut r, 12, AUTH, PASSWORD);
    rcon.request(&mut r, 13, AUTH, "wrong");
    rcon.request(&mut r, 14, COMMAND, "list");
    rcon.request(&mut r, 15, AUTH, PASSWORD);
    for (id, command) in [
        (20, "list"),
        (21, "/list"),
        (22, "say hello rcon"),
        (23, "say h\u{e9}llo \u{20ac} \u{1f600}"),
        (24, "give Nobody minecraft:stone"),
        (25, "foo"),
        (26, "time set 1000"),
        (27, "gamerule random_tick_speed"),
        (28, "data get entity @e"),
        (29, "execute as @s run say as the source"),
        (30, "execute if entity @s"),
        (31, ""),
        (32, " "),
        (33, "say"),
        (34, "tell Nobody hi"),
        (35, "kill @e[type=item]"),
        (36, "difficulty"),
        (37, "seed"),
    ] {
        rcon.request(&mut r, id, COMMAND, command);
    }
    for (id, kind) in [(40, 9), (41, 0), (42, 100), (43, -5), (44, 1), (45, 4)] {
        rcon.request(&mut r, id, kind, "x");
    }

    // a surrogate pair at the end of the first packet of an answer: `data get storage` prints the four strings
    for (id, key) in [(50, 'a'), (51, 'b'), (52, 'c')] {
        let text = key.to_string().repeat(1000);
        rcon.request(
            &mut r,
            id,
            COMMAND,
            &format!(r#"data modify storage probe:s {key} set value "{text}""#),
        );
    }
    rcon.request(
        &mut r,
        53,
        COMMAND,
        r#"data modify storage probe:s d set value "XQX""#,
    );
    let answer = rcon.request(&mut r, 54, COMMAND, "data get storage probe:s");
    r.out.push(format!(
        "  packets of the long answer: {:?}",
        answer
            .iter()
            .map(|p| p.text.encode_utf16().count())
            .collect::<Vec<_>>()
    ));
    let at = answer
        .first()
        .and_then(|p| {
            p.text
                .find("XQX")
                .map(|b| p.text[..b].encode_utf16().count())
        })
        .expect("the string is in the answer");
    for (id, from) in [(60, 4094), (70, 4095)] {
        let pad = "d".repeat(from - at);
        rcon.request(
            &mut r,
            id,
            COMMAND,
            &format!("data modify storage probe:s d set value \"{pad}\u{1f600}b\""),
        );
        let long = rcon.request(&mut r, id + 1, COMMAND, "data get storage probe:s");
        r.out.push(format!(
            "  bytes around the end of the first packet and the start of the second: {:?} {:?}",
            long.first()
                .map(|p| p.raw[p.raw.len().saturating_sub(6)..].to_vec()),
            long.get(1).map(|p| p.raw[..p.raw.len().min(6)].to_vec()),
        ));
    }

    // packets the server drops the connection for
    for (what, bytes) in [
        ("fewer than ten bytes", vec![5, 0, 0, 0, 1, 0, 0, 0, 2]),
        (
            "a length that does not match the read",
            [
                9i32.to_le_bytes(),
                1i32.to_le_bytes(),
                COMMAND.to_le_bytes(),
            ]
            .concat()
            .into_iter()
            .chain([0, 0])
            .collect(),
        ),
        (
            "two packets in one write",
            [
                Rcon::frame(1, AUTH, PASSWORD.as_bytes()),
                Rcon::frame(2, COMMAND, b"list"),
            ]
            .concat(),
        ),
        (
            "a command of 2000 bytes",
            Rcon::frame(3, COMMAND, "say a".repeat(400).as_bytes()),
        ),
    ] {
        r.step(&format!("a new connection: {what}"));
        let mut other = Rcon::connect(s);
        other.write(&bytes);
        other.report(&mut r);
    }
    r.step("authenticate, then a command of 1450 bytes");
    let mut other = Rcon::connect(s);
    other.login(&mut r);
    other.request(&mut r, 3, COMMAND, &format!("say {}", "a".repeat(1440)));
    tokio::time::sleep(SETTLE).await;
    r.log();
    r.out
}

/// The command source of RCON is the overworld at the respawn position, however the spawn moved.
async fn source(s: &Server) -> Vec<String> {
    let mut rcon = Rcon::connect(s);
    let mut r = Rec::new(s);
    r.log();
    rcon.login(&mut r);
    // the markers live in loaded chunks around the origin of each dimension
    for dimension in ["overworld", "the_nether", "the_end"] {
        rcon.request(
            &mut r,
            2,
            COMMAND,
            &format!("execute in minecraft:{dimension} run forceload add -48 -48 48 48"),
        );
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    let place_marker = |rcon: &mut Rcon, r: &mut Rec<'_>, id: i32| {
        rcon.request(r, id, COMMAND, r#"summon marker ~ ~ ~ {Tags:["probe"]}"#);
        for dimension in ["overworld", "the_nether"] {
            for what in ["Pos", "Rotation"] {
                rcon.request(
                    r,
                    id + 1,
                    COMMAND,
                    &format!(
                        "execute in minecraft:{dimension} run data get entity @e[type=marker,tag=probe,limit=1] {what}"
                    ),
                );
            }
        }
        rcon.request(r, id + 2, COMMAND, "kill @e[type=marker,tag=probe]");
        rcon.request(
            r,
            id + 3,
            COMMAND,
            "execute in minecraft:the_nether run kill @e[type=marker,tag=probe]",
        );
    };
    place_marker(&mut rcon, &mut r, 10);
    rcon.request(
        &mut r,
        20,
        COMMAND,
        "execute in minecraft:the_nether run setworldspawn 10 70 -12",
    );
    place_marker(&mut rcon, &mut r, 30);
    rcon.request(&mut r, 40, COMMAND, "setworldspawn 3 71 4");
    place_marker(&mut rcon, &mut r, 50);
    rcon.request(
        &mut r,
        60,
        COMMAND,
        "execute in minecraft:the_end run setworldspawn 5 72 6",
    );
    place_marker(&mut rcon, &mut r, 70);
    for command in [
        "execute if dimension minecraft:overworld",
        "execute if dimension minecraft:the_nether",
        "execute positioned 0 0 0 run setworldspawn",
        "execute as @s at @s run data get entity @s",
        "execute store result score @s x run say hello",
    ] {
        rcon.request(&mut r, 80, COMMAND, command);
    }
    r.out
}

/// What an operator is told of the commands RCON runs.
async fn broadcast(s: &Server) -> Vec<String> {
    let bot = crate::common::join("127.0.0.1", s.port, "Bot").await;
    let mut r = Rec::new(s);
    r.log();
    r.step("op Bot");
    s.cmd("op Bot");
    bot.wait_ticks(20).await.expect("ticking");
    r.log();
    let mut rcon = Rcon::connect(s);
    rcon.login(&mut r);
    let before = bot.with_state(|st| st.chat.len());
    for (id, command) in [
        (2, "time set 1000"),
        (3, "difficulty hard"),
        (4, "gamemode creative Bot"),
        (5, "say hi from rcon"),
        (6, "weather clear"),
        (7, "foo"),
        (8, "give Nobody stone"),
    ] {
        rcon.request(&mut r, id, COMMAND, command);
    }
    bot.wait_ticks(20).await.expect("ticking");
    let told: Vec<String> = bot.with_state(|st| {
        st.chat[before..]
            .iter()
            .map(|c| format!("{} {}", c.kind, c.text))
            .collect()
    });
    r.out.push(format!("the operator was told: {told:#?}"));
    bot.disconnect().await;
    r.out
}

async fn broadcast_true(s: &Server) -> Vec<String> {
    broadcast(s).await
}

async fn broadcast_false(s: &Server) -> Vec<String> {
    broadcast(s).await
}

// ---- files -----------------------------------------------------------------------------------------------------

/// A JSON file of the server with the dates a later run would differ in masked.
fn file(s: &Server, name: &str) -> String {
    let text = std::fs::read_to_string(s.dir().join(name)).unwrap_or_else(|_| "<missing>".into());
    let mut out = text;
    for key in [r#""created": ""#, r#""expiresOn":""#] {
        let mut masked = String::new();
        let mut rest = out.as_str();
        while let Some(at) = rest.find(key) {
            let start = at + key.len();
            let end = rest[start..].find('"').unwrap() + start;
            masked.push_str(&rest[..start]);
            masked.push_str("<date>");
            rest = &rest[end..];
        }
        masked.push_str(rest);
        out = masked;
    }
    out
}

/// The files written after a login, user list changes and a stop.
async fn files(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.log();
    let _bot = crate::common::join("127.0.0.1", s.port, "Bot").await;
    for command in [
        "op Bot",
        "whitelist add Bot",
        "ban-ip 5.6.7.8 spam",
        "pardon-ip 5.6.7.8",
        "ban-ip 6.7.8.9 other",
        "ban Bot a reason",
        "save-all flush",
    ] {
        r.console(command);
        tokio::time::sleep(Duration::from_millis(700)).await;
        r.log();
    }
    tokio::time::sleep(SETTLE).await;
    r.step("after the stop");
    s.stop();
    for name in [
        "usercache.json",
        "ops.json",
        "whitelist.json",
        "banned-players.json",
        "banned-ips.json",
    ] {
        r.out.push(format!("{name}: {}", file(s, name)));
    }
    r.out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn rcon_dual_vanilla() {
    record_vanilla(DIR, ONLY, CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn rcon_dual_ours() {
    compare_ours(DIR, ONLY, CASES).await;
}
