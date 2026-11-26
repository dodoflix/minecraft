//! A connection in the login phase is ticked by the server tick: a client that sends only `hello` is disconnected
//! with `multiplayer.disconnect.slow_login` after 600 server ticks, however fast those ticks run (`/tick sprint 700`
//! runs them in a fraction of the 30 s the wall clock would need). What the client receives and the server thread's log
//! lines equal vanilla's, recorded in `tests/fixtures/login_tick_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them).
use std::time::Duration;

use crate::common::dual::PROPS;
use crate::kick_dual::{Case, Rec, T, case, compare_ours, mask_ports, record_vanilla};
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::packets::handshake::{
    ClientIntent, ClientIntentionPacket, ServerboundHandshakePacket,
};
use mc_protocol::packets::login::{
    ClientboundLoginPacket, ServerboundHelloPacket, ServerboundLoginPacket,
};
use mc_testbot::conn::Conn;
use mc_testbot::join::BotOptions;
use mc_testkit::Server;

const CASES: &[Case] = &[
    case!(hello_only, PROPS),
    case!(hello_only_at_a_low_tick_rate, PROPS),
];

/// A connection that sent the handshake and `hello`, and has read the packets up to `login_finished`.
async fn hello(s: &Server, r: &mut Rec<'_>) -> Conn {
    let mut conn = Conn::connect("127.0.0.1", s.port, T, Default::default())
        .await
        .expect("connect");
    let options = BotOptions::new("127.0.0.1", s.port, "Bot");
    conn.send(&ServerboundHandshakePacket::ClientIntention(
        ClientIntentionPacket {
            protocol_version: PROTOCOL_VERSION,
            host_name: "127.0.0.1".into(),
            port: s.port,
            intention: ClientIntent::Login,
        },
    ))
    .await
    .expect("handshake");
    conn.send(&ServerboundLoginPacket::Hello(ServerboundHelloPacket {
        name: options.username.clone(),
        profile_id: options.profile_uuid(),
    }))
    .await
    .expect("hello");
    loop {
        match conn.recv::<ClientboundLoginPacket>().await {
            Ok(ClientboundLoginPacket::LoginCompression(p)) => {
                r.out
                    .push(format!("login_compression {}", p.compression_threshold));
                conn.reader.set_compression(p.compression_threshold, true);
                conn.writer.set_compression(p.compression_threshold);
            }
            Ok(ClientboundLoginPacket::LoginFinished(f)) => {
                r.out
                    .push(format!("login_finished {}", f.game_profile.name));
                return conn;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// The `lost connection` lines of the server, with the reason: the reason a client sees in `log_message`'s masked form.
fn closing(s: &Server) -> Vec<String> {
    s.log()
        .lines()
        .filter_map(|l| l.split_once("]: ").map(|(_, text)| text))
        .filter(|text| text.contains("lost connection: "))
        .map(|text| format!("  raw: {}", mask_ports(text)))
        .collect()
}

/// The client sends `hello` and nothing else: the server waits for `login_acknowledged` until the 600th tick.
async fn hello_only(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    let mut conn = hello(s, &mut r).await;
    r.console("tick sprint 700");
    let answer = match conn.recv::<ClientboundLoginPacket>().await {
        Err(e) => format!("error: {e}"),
        Ok(other) => format!("unexpected {other:?}"),
    };
    r.out.push(answer);
    tokio::time::sleep(Duration::from_secs(2)).await;
    r.log();
    r.out.extend(closing(s));
    r.out
}

/// With one tick per second 600 ticks take ten minutes: the same client is still connected after a few seconds, and is
/// disconnected only once the ticks are fast again.
async fn hello_only_at_a_low_tick_rate(s: &Server) -> Vec<String> {
    let mut r = Rec::new(s);
    r.ordered = true;
    r.console("tick rate 1");
    let mut conn = hello(s, &mut r).await;
    r.step("wait 4 s");
    let waited = tokio::time::timeout(
        Duration::from_secs(4),
        conn.recv::<ClientboundLoginPacket>(),
    )
    .await;
    r.out.push(match waited {
        Err(_) => "still connected".to_owned(),
        Ok(answer) => format!("closed early: {answer:?}"),
    });
    r.console("tick rate 20");
    r.console("tick sprint 700");
    r.out
        .push(match conn.recv::<ClientboundLoginPacket>().await {
            Err(e) => format!("error: {e}"),
            Ok(other) => format!("unexpected {other:?}"),
        });
    tokio::time::sleep(Duration::from_secs(2)).await;
    r.log();
    r.out.extend(closing(s));
    r.out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn login_tick_dual_vanilla() {
    record_vanilla("login_tick_dual", "LOGIN_TICK_DUAL_ONLY", CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn login_tick_dual_ours() {
    compare_ours("login_tick_dual", "LOGIN_TICK_DUAL_ONLY", CASES).await;
}
