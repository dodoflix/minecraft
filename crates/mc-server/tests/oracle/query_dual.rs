//! The GameSpy-4 query listener (`enable-query`, `query.port`), observed with the same UDP datagrams on the real
//! vanilla server and on `mc-server`: the handshake, the basic and the full stat (with no player and with one, the 5 s
//! cache of the full stat), tokens (wrong, replaced, two source ports, an old one), datagrams that are too short, too
//! long, of an unknown type or without the magic, the ident bytes a short type-9 datagram takes from the previous
//! datagram, idents that are not UTF-8, and the log lines of the listener. Replies are compared byte for byte; the
//! challenge token and the game port are masked. The configuration rows start a server per `query.port` value.
//!
//! `matrix_equals_vanilla` takes about 50 s per server (the old-token row waits 31 s), `config_rows_equal_vanilla` about
//! 20 s per row.

use std::fmt::Write as _;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use crate::common::join;
use crate::server_properties_dual::{mask_ports, message, nothing, on_both};
use mc_testkit::{Answer, QueryClient, Server, free_port};

const MAGIC: [u8; 2] = [0xFE, 0xFD];
const FLAT: [&str; 2] = ["level-type=minecraft:flat", "generate-structures=false"];

/// Readable bytes: printable ASCII as it is, NUL as `\0`, everything else `\xNN`.
fn show(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        match b {
            0 => out.push_str("\\0"),
            0x20..=0x7e => out.push(*b as char),
            _ => {
                let _ = write!(out, "\\x{b:02x}");
            }
        }
        out
    })
}

/// A reply with what differs between the servers masked: the challenge token of a handshake reply and the game port
/// of a stat reply (two bytes in the basic stat, a decimal string in the full stat).
fn view(reply: &[u8], port: u16) -> String {
    let nul_strings = |from: usize, n: usize| {
        let mut at = from;
        for _ in 0..n {
            at += reply.get(at..)?.iter().position(|b| *b == 0)? + 1;
        }
        Some(at)
    };
    match reply.first() {
        Some(9) => {
            let end = reply.len() - 1;
            let digits = reply[..end]
                .iter()
                .rev()
                .take_while(|b| b.is_ascii_digit())
                .count();
            format!(
                "{}<token>{}",
                show(&reply[..end - digits]),
                show(&reply[end..])
            )
        }
        Some(0) if reply.get(5..).is_some_and(|r| r.starts_with(b"splitnum")) => {
            show(reply).replacen(&format!("hostport\\0{port}\\0"), "hostport\\0<port>\\0", 1)
        }
        Some(0) => match nul_strings(5, 5) {
            Some(at) if reply.get(at..at + 2) == Some(&port.to_le_bytes()[..]) => {
                format!("{}<port>{}", show(&reply[..at]), show(&reply[at + 2..]))
            }
            _ => show(reply),
        },
        _ => show(reply),
    }
}

/// The token of a handshake reply (`\t`, ident, decimal token, NUL); idents in this file never end in a digit.
fn token(reply: &[u8]) -> i32 {
    let text = &reply[..reply.len() - 1];
    let digits = text.iter().rev().take_while(|b| b.is_ascii_digit()).count();
    String::from_utf8_lossy(&text[text.len() - digits..])
        .parse()
        .expect("a token")
}

fn type_9(ident: &[u8]) -> Vec<u8> {
    [&MAGIC[..], &[9], ident].concat()
}

/// A stat request of `len` bytes: the type 0 packet with its ident and token, cut or padded with zeros. 15 bytes is
/// the full stat.
fn stat(ident: &[u8; 4], token: i32, len: usize) -> Vec<u8> {
    let mut d = [&MAGIC[..], &[0], ident, &token.to_be_bytes()].concat();
    d.resize(len, 0);
    d
}

fn ask(client: &QueryClient, datagram: &[u8], port: u16) -> String {
    match client.send(datagram) {
        Answer::Packet(reply) => view(&reply, port),
        Answer::Silent => "no reply".to_owned(),
        Answer::Refused => "port closed".to_owned(),
    }
}

/// Sends all datagrams at once and reports every reply that follows within 300 ms; for rows that have none.
fn burst(client: &QueryClient, datagrams: &[Vec<u8>], port: u16) -> String {
    for d in datagrams {
        client.send_only(d);
    }
    let mut replies = Vec::new();
    while let Answer::Packet(reply) = client.recv(Duration::from_millis(300)) {
        replies.push(view(&reply, port));
    }
    if replies.is_empty() {
        "no reply".to_owned()
    } else {
        replies.join(" | ")
    }
}

fn handshake(client: &QueryClient, ident: &[u8; 4]) -> (i32, Vec<u8>) {
    match client.send(&type_9(ident)) {
        Answer::Packet(reply) => (token(&reply), reply),
        other => panic!("no handshake reply: {other:?}"),
    }
}

/// The lines about the query listener, in the order they sort (its thread and the server thread race).
fn query_lines(server: &Server) -> Vec<String> {
    let mut lines: Vec<String> = server
        .log()
        .lines()
        .filter_map(message)
        .filter(|m| m.contains("uery") || m.contains("GS4"))
        .map(|m| mask_ports(&m))
        .collect();
    lines.sort();
    lines
}

/// Waits until a basic stat shows `count` players. Vanilla reads the player list per request; our server publishes it
/// once per tick, so a request right after a join or a leave may still see the old count.
async fn await_players(client: &QueryClient, request: &[u8], count: &str) {
    for _ in 0..100 {
        if let Answer::Packet(reply) = client.send(request) {
            let mut fields = reply[5..].split(|b| *b == 0);
            if fields.nth(3) == Some(count.as_bytes()) {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the query never showed {count} players");
}

async fn sleep_until(at: Instant) {
    tokio::time::sleep(at.saturating_duration_since(Instant::now())).await;
}

struct Rows(Vec<String>);

impl Rows {
    fn add(&mut self, label: &str, text: impl std::fmt::Display) {
        self.0.push(format!("{label}: {text}"));
    }
}

async fn matrix(server: &Server, query_port: u16) -> Vec<String> {
    let port = server.port;
    let mut rows = Rows(Vec::new());
    // the datagrams of one source: the challenge table is keyed by its address
    let a = QueryClient::connect(query_port);
    let abcd = b"ABCD";

    // the very first datagram of the listener: the bytes it did not overwrite are zeros
    rows.add("FE FD 09 on a fresh buffer", ask(&a, &type_9(&[]), port));
    let (_, reply) = handshake(&a, abcd);
    rows.add("handshake", view(&reply, port));
    let old = QueryClient::connect(query_port);
    let old_since = Instant::now();
    let (old_token, _) = handshake(&old, b"OLDX");

    // basic stat, wrong token, replaced token
    let (token_a, _) = handshake(&a, abcd);
    rows.add("basic stat", ask(&a, &stat(abcd, token_a, 11), port));
    let (token_a2, _) = handshake(&a, abcd);
    assert_ne!(token_a, token_a2, "two handshakes drew the same token");
    rows.add(
        "token of the replaced challenge",
        ask(&a, &stat(abcd, token_a, 11), port),
    );
    rows.add("wrong token", ask(&a, &stat(abcd, token_a2 ^ 1, 11), port));
    rows.add("current token", ask(&a, &stat(abcd, token_a2, 11), port));
    let b = QueryClient::connect(query_port);
    rows.add(
        "stat without a handshake",
        ask(&b, &stat(b"NOPE", 0, 11), port),
    );
    rows.add(
        "full stat without a handshake",
        ask(&b, &stat(b"NOPE", 0, 15), port),
    );

    // datagrams that are not answered
    let typed = |t: u8, len: usize| {
        let mut d = [&MAGIC[..], &[t], abcd, &token_a2.to_be_bytes()].concat();
        d.resize(len, 0);
        d
    };
    for t in [1u8, 7, 8, 10, 255] {
        rows.add(&format!("type {t}"), burst(&a, &[typed(t, 15)], port));
    }
    for (label, magic) in [
        ("FE FC", [0xFE, 0xFC]),
        ("FD FE", [0xFD, 0xFE]),
        ("00 00", [0, 0]),
        ("FF FF", [0xFF, 0xFF]),
    ] {
        let d = [&magic[..], &[9], abcd].concat();
        rows.add(&format!("magic {label}"), burst(&a, &[d], port));
    }
    let shorts: Vec<Vec<u8>> = (0..3)
        .map(|n| stat(abcd, token_a2, 11)[..n].to_vec())
        .collect();
    rows.add("datagrams of 0, 1 and 2 bytes", burst(&a, &shorts, port));
    let shorts: Vec<Vec<u8>> = (3..11).map(|n| stat(abcd, token_a2, n)).collect();
    rows.add("stat of 3 to 10 bytes", burst(&a, &shorts, port));

    // every length that is not 15 is the basic stat
    for len in (11..15).chain(16..41) {
        rows.add(
            &format!("stat of {len} bytes"),
            ask(&a, &stat(abcd, token_a2, len), port),
        );
    }

    // the full stat with nobody online starts the 5 s cache
    let full = stat(abcd, token_a2, 15);
    rows.add("full stat, no player", ask(&a, &full, port));
    let cached_since = Instant::now();
    rows.add("full stat again", ask(&a, &full, port));

    // two sources hold independent tokens
    let c = QueryClient::connect(query_port);
    let (token_c, _) = handshake(&c, b"IJKL");
    rows.add(
        "the token of another source",
        ask(&c, &stat(b"IJKL", token_a2, 11), port),
    );
    rows.add(
        "own token, second source",
        ask(&c, &stat(b"IJKL", token_c, 11), port),
    );
    rows.add(
        "own token, first source",
        ask(&a, &stat(abcd, token_a2, 11), port),
    );

    // longer than the 1 460 byte buffer
    let d = QueryClient::connect(query_port);
    let long = [&MAGIC[..], &[9], b"MNOP", &[b'Z'; 1454]].concat();
    let (token_d, reply) = match d.send(&long) {
        Answer::Packet(reply) => (token(&reply), view(&reply, port)),
        other => panic!("no reply to a 1461 byte handshake: {other:?}"),
    };
    rows.add("type 9 of 1461 bytes", reply);
    for len in [1459, 1460, 1461, 2000] {
        rows.add(
            &format!("stat of {len} bytes"),
            ask(&d, &stat(b"MNOP", token_d, len), port),
        );
    }

    // a type-9 datagram of 3 to 6 bytes takes the rest of its ident from the previous datagram
    let e = QueryClient::connect(query_port);
    rows.add(
        "type 7 that leaves WXYZ",
        burst(&e, &[[&MAGIC[..], &[7], b"WXYZ", &[0; 20]].concat()], port),
    );
    let shortened = type_9(b"abcd");
    for len in 3..=7 {
        rows.add(
            &format!("type 9 of {len} bytes"),
            ask(&e, &shortened[..len], port),
        );
    }
    // ...and idents that are not UTF-8 decode like Java's `new String(bytes, UTF_8)`
    let idents: [&[u8; 4]; 12] = [
        b"\xed\xa0\x80k",
        b"\xff\xfe\xfd\x80",
        b"\xe2\x82AB",
        b"\xc0\x80AB",
        b"\xf0\x9f\x98A",
        b"\xf4\x90\x80\x80",
        b"\xe0\x80\x80A",
        b"\x80\x80\x80\x80",
        b"\xf0\x9f\x98\x80",
        b"\xe2\x82\xac!",
        b"\xc3\x28\xa0\xa1",
        b"AB\xe2\x82",
    ];
    for ident in idents {
        rows.add(
            &format!("ident {}", show(ident)),
            ask(&e, &type_9(ident), port),
        );
    }
    // the same bytes taken from the buffer instead of the datagram
    for ident in [b"\xed\xa0\x80k", b"\xe2\x82AB"] {
        burst(&e, &[[&MAGIC[..], &[7], ident, &[0; 4]].concat()], port);
        rows.add(
            &format!("stale ident {}", show(ident)),
            ask(&e, &type_9(&[]), port),
        );
    }

    // one player: the basic stat is live, the full stat comes from the cache after 5 s
    let bot = join("127.0.0.1", port, "Bot").await;
    sleep_until(cached_since + Duration::from_millis(5500)).await;
    let basic = stat(abcd, token_a2, 11);
    await_players(&a, &basic, "1").await;
    rows.add("basic stat, one player", ask(&a, &basic, port));
    rows.add("full stat, one player", ask(&a, &full, port));
    let full_since = Instant::now();
    bot.disconnect().await;
    server
        .wait_log("Bot left the game", 30)
        .expect("the bot left");
    await_players(&a, &basic, "0").await;
    let f = QueryClient::connect(query_port);
    let (token_f, _) = handshake(&f, b"EFGH");
    assert!(
        full_since.elapsed() < Duration::from_millis(4500),
        "the cache window passed before the cached reply"
    );
    rows.add(
        "full stat inside the cache window, new ident",
        ask(&f, &stat(b"EFGH", token_f, 15), port),
    );
    rows.add("basic stat, nobody online", ask(&a, &basic, port));
    sleep_until(full_since + Duration::from_millis(5500)).await;
    rows.add("full stat after the window", ask(&a, &full, port));

    // challenges are not dropped by the prune that runs 30 s after the listener started
    sleep_until(old_since + Duration::from_secs(31)).await;
    rows.add(
        "stat with a 31 s old token",
        ask(&old, &stat(b"OLDX", old_token, 11), port),
    );

    server.stop();
    server
        .wait_log("Thread Query Listener stopped", 30)
        .expect("the listener stopped");
    for line in query_lines(server) {
        rows.add("log", line);
    }
    rows.0
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn matrix_equals_vanilla() {
    let query_port = free_port();
    let props = [
        FLAT[0],
        FLAT[1],
        "enable-query=true",
        &format!("query.port={query_port}"),
        "motd=Query Dual",
        "max-players=7",
        "level-name=qdual",
    ];
    on_both(&props, nothing, async |server, _| {
        matrix(server, query_port).await
    })
    .await;
}

/// Servers whose query listener does not start: the log lines and what a handshake gets.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn config_rows_equal_vanilla() {
    for (label, enable, port) in [
        ("disabled", "false", free_port().to_string()),
        ("port 0", "true", "0".to_owned()),
        ("port 70000", "true", "70000".to_owned()),
        ("port -5", "true", "-5".to_owned()),
        ("port in use", "true", free_port().to_string()),
    ] {
        println!("== {label}");
        // the occupied port is held until both servers ran
        let held = (label == "port in use").then(|| {
            UdpSocket::bind(("127.0.0.1", port.parse().unwrap())).expect("hold the query port")
        });
        let enable = format!("enable-query={enable}");
        let query_port = format!("query.port={port}");
        let props = [FLAT[0], FLAT[1], enable.as_str(), query_port.as_str()];
        on_both(&props, nothing, async |server, _| {
            let mut rows = Rows(Vec::new());
            if label == "disabled" {
                // nothing listens: the host answers with ICMP port unreachable
                let client = QueryClient::connect(port.parse().unwrap());
                rows.add("handshake", ask(&client, &type_9(b"ABCD"), server.port));
            }
            server.stop();
            for line in query_lines(server) {
                rows.add("log", line);
            }
            rows.0
        })
        .await;
        drop(held);
    }
}

/// Without `server-ip` the listener binds every interface and reports the host's own address (`getLocalHost`: the
/// host name's first IPv4 address).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn empty_server_ip_reports_the_local_host_address() {
    let query_port = free_port();
    let props = [
        FLAT[0],
        FLAT[1],
        "server-ip=",
        "enable-query=true",
        &format!("query.port={query_port}"),
        "motd=Query Dual",
        "level-name=qdual",
    ];
    on_both(&props, nothing, async |server, _| {
        let mut rows = Rows(Vec::new());
        let client = QueryClient::connect(query_port);
        let (token, _) = handshake(&client, b"ABCD");
        rows.add(
            "basic stat",
            ask(&client, &stat(b"ABCD", token, 11), server.port),
        );
        server.stop();
        server
            .wait_log("Thread Query Listener stopped", 30)
            .expect("the listener stopped");
        for line in query_lines(server) {
            rows.add("log", line);
        }
        rows.0
    })
    .await;
}
