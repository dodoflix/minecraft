//! Online mode against the mock Mojang services (`mc_testkit::MockMojang`), the SAME bot script on the vanilla server and on
//! OUR server: the bot calls the session `join` endpoint named by the discovery document, the server's own `hasJoined`
//! request goes to the same document's `verify` endpoint. Both sides must produce the same server log lines about the
//! player, the same requests to the services (modulo the per-run server hash) and the same outcome for the client.
//!
//! Each test starts the two servers one after the other (never together) and compares their observations.
use std::time::Duration;

use mc_testbot::join::BotOptions;
use mc_testbot::{Bot, BotError};
use mc_testkit::{MockMojang, Server};

const PROPS: &[&str] = &["online-mode=true", "enforce-secure-profile=true"];

#[derive(Clone, Copy, Debug)]
enum Side {
    Vanilla,
    Ours,
}

fn start(side: Side, mock: &MockMojang) -> Server {
    let builder = match side {
        Side::Vanilla => mc_testbot::harness::vanilla(12345, "creative", PROPS),
        Side::Ours => super::common::ours(12345, "creative", PROPS),
    };
    builder.mojang(mock).start().expect("start the server")
}

/// The messages of the log lines that mention `name` or the authentication, without time, thread, ports and the
/// entity id of the login line.
fn log_lines(server: &Server, name: &str) -> Vec<String> {
    server
        .log()
        .lines()
        .filter_map(|l| {
            let (head, message) = l.split_once("]: ")?;
            let level = head.rsplit('/').next()?;
            // whether the bot's close or the server's disconnect is seen first decides if `lost connection` is logged
            (message.contains(name) && !message.contains("lost connection")).then(|| {
                let message = match message.split_once("] logged in with entity id") {
                    Some((who, _)) => format!("{who}] logged in"),
                    None => message.to_string(),
                };
                let message = mask_address(&message);
                format!("{level} {message}")
            })
        })
        .collect()
}

/// `Name[/127.0.0.1:41234]` and `Name (/127.0.0.1:41234)` lose their address.
fn mask_address(message: &str) -> String {
    let mut out = String::new();
    let mut rest = message;
    while let Some(at) = rest.find("/127.0.0.1:") {
        let (head, tail) = rest.split_at(at);
        let end = tail.find([']', ')']).unwrap_or(tail.len());
        out.push_str(head);
        out.push_str("/<address>");
        rest = &tail[end..];
    }
    out + rest
}

/// What the services saw: `METHOD path?query-keys` with the values of `serverId` masked.
fn service_requests(mock: &MockMojang) -> Vec<String> {
    mock.requests()
        .into_iter()
        .filter(|r| r.target.starts_with("/session/"))
        .map(|r| {
            let (path, query) = r.target.split_once('?').unwrap_or((&r.target, ""));
            let query: Vec<String> = query
                .split('&')
                .filter(|q| !q.is_empty())
                .map(|q| match q.split_once('=') {
                    Some(("serverId", _)) => "serverId=<hash>".to_string(),
                    _ => q.to_string(),
                })
                .collect();
            format!("{} {path}?{}", r.method, query.join("&"))
        })
        .collect()
}

async fn observe(
    side: Side,
    run: impl AsyncFnOnce(&MockMojang, &Server) -> Vec<String>,
) -> Vec<String> {
    let mock = MockMojang::start().expect("start the mock services");
    let server = start(side, &mock);
    let mut out = run(&mock, &server).await;
    out.extend(
        service_requests(&mock)
            .into_iter()
            .map(|r| format!("mock: {r}")),
    );
    server.stop();
    out
}

async fn dual(run: impl AsyncFn(&MockMojang, &Server) -> Vec<String>) {
    let vanilla = observe(Side::Vanilla, &run).await;
    let ours = observe(Side::Ours, &run).await;
    assert_eq!(
        vanilla, ours,
        "vanilla (left) and our server (right) differ"
    );
}

fn options(server: &Server, mock: &MockMojang, name: &str) -> BotOptions {
    let mut options =
        BotOptions::new("127.0.0.1", server.port, name).session(mock.discovery_url(), name);
    options.timeout = Duration::from_secs(60);
    options
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn online_login_through_the_mock_services() {
    dual(async |mock, server| {
        let name = "OnlineBot";
        let bot = Bot::connect(options(server, mock, name))
            .await
            .expect("online login");
        bot.wait_joined(Duration::from_secs(60))
            .await
            .expect("play");
        let profile =
            bot.with_state(|s| (s.configured.profile_name.clone(), s.configured.profile_id));
        server
            .wait_log(&format!("{name} joined the game"), 30)
            .expect("joined");
        let mut out = vec![format!(
            "client: profile {} {} online={}",
            profile.0,
            profile.1 == mc_testkit::offline_uuid(name),
            mock.joins().len() == 1
        )];
        out.extend(
            log_lines(server, name)
                .into_iter()
                .map(|l| format!("log: {l}")),
        );
        out
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn a_join_for_another_profile_is_refused() {
    dual(async |mock, server| {
        let name = "Mallory";
        let mut options = options(server, mock, name);
        options.uuid = Some(uuid::Uuid::from_u128(0x1234));
        let Err(BotError::Disconnected(reason)) = Bot::connect(options).await else {
            panic!("expected a disconnect")
        };
        let mut out = vec![format!("client: disconnected {reason}")];
        out.extend(
            log_lines(server, name)
                .into_iter()
                .map(|l| format!("log: {l}")),
        );
        out
    })
    .await;
}
