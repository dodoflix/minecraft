//! Online mode against the mock Mojang services (`mc_testkit::MockMojang`), the SAME bot script on the vanilla server and on
//! OUR server: the bot calls the session `join` endpoint named by the discovery document, the server's own `hasJoined`
//! request goes to the same document's `verify` endpoint. Both sides must produce the same server log lines about the
//! player, the same requests to the services (modulo the per-run server hash) and the same outcome for the client.
//!
//! Each test starts the two servers one after the other (never together) and compares their observations.
use std::sync::Arc;
use std::time::Duration;

use mc_testbot::bot::ChatFault;
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
            ((message.contains(name) || message.contains("Received expired chat"))
                && !message.contains("lost connection"))
            .then(|| {
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

// ---- signed chat ---------------------------------------------------------------------------------------------------

/// An online bot; with `session` its chat session (`ServerboundChatSessionUpdate`) is sent too.
async fn chat_bot(server: &Server, mock: &MockMojang, name: &str, session: bool) -> Arc<Bot> {
    let bot = Bot::connect(options(server, mock, name))
        .await
        .expect("online login");
    bot.wait_joined(Duration::from_secs(60))
        .await
        .expect("play");
    bot.wait_ready(Duration::from_secs(90))
        .await
        .expect("loaded");
    server
        .wait_log(&format!("{name} joined the game"), 30)
        .expect("joined");
    if session {
        bot.start_chat_session().await.expect("chat session");
        // the server broadcasts the session (`INITIALIZE_CHAT`) once it has accepted it
        poll(WAIT, || {
            bot.log_text().lines().any(|l| {
                l.contains("<- play/player_info_update") && l.contains("chat_session: Some")
            })
        })
        .await;
    }
    Arc::new(bot)
}

/// The chat the bot received (kind and text, joins left out) and how many of its player chat packets were signed.
fn received(bot: &Bot) -> Vec<String> {
    let mut out: Vec<String> = bot.with_state(|s| {
        s.chat
            .iter()
            .filter(|l| !l.text.contains("joined the game"))
            .map(|l| format!("{} {}", l.kind, l.text))
            .collect()
    });
    let signed = bot
        .log_text()
        .lines()
        .filter(|l| l.contains("<- play/player_chat") && l.contains("signature: Some"))
        .count();
    out.push(format!("signed player_chat packets: {signed}"));
    out
}

fn section(title: &str, lines: impl IntoIterator<Item = String>) -> Vec<String> {
    lines.into_iter().map(|l| format!("{title}: {l}")).collect()
}

const WAIT: Duration = Duration::from_secs(30);

/// Waits until `condition` holds.
async fn poll(timeout: Duration, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + timeout;
    while !condition() {
        assert!(std::time::Instant::now() < deadline, "timed out");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Waits until the server log has `count` lines containing `needle`: the log is written after the client saw the reply.
async fn logged(server: &Server, needle: &str, count: usize) {
    poll(WAIT, || {
        server.log().lines().filter(|l| l.contains(needle)).count() >= count
    })
    .await;
}

/// Says `text` and waits until the bot's own chat shows `expect`.
async fn say(bot: &Bot, text: &str, fault: ChatFault, expect: &str) {
    let since = bot.chat_len();
    bot.chat_with(text, fault);
    if let Err(e) = bot.wait_chat(expect, since, WAIT).await {
        panic!("{expect}: {e}; chat so far: {:?}", received(bot));
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn signed_chat_and_signed_command_arguments_are_broadcast() {
    dual(async |mock, server| {
        let alice = chat_bot(server, mock, "Alice", true).await;
        let bob = chat_bot(server, mock, "Bob", true).await;
        let since = bob.chat_len();
        alice.chat("hello");
        bob.wait_chat("hello", since, WAIT).await.expect("hello");
        let since = bob.chat_len();
        alice.command_signed_args("msg Bob hi there", &[("message", "hi there")]);
        bob.wait_chat("hi there", since, WAIT).await.expect("msg");
        // a long chain: the acknowledgements keep the server's window in step (operators are exempt from the spam kick)
        server.cmd("op Alice");
        server
            .wait_log("Made Alice a server operator", 30)
            .expect("op");
        let since = bob.chat_len();
        for i in 0..100 {
            alice.chat(&format!("chain {i}"));
        }
        if let Err(e) = bob.wait_chat("chain 99", since, WAIT).await {
            let got = received(&bob);
            panic!(
                "chain 99: {e}; bob got {} lines, last {:?}; alice {:?}; log {:?}",
                got.len(),
                &got[got.len().saturating_sub(4)..],
                alice.assert_connected().err().map(|e| e.to_string()),
                server
                    .log()
                    .lines()
                    .filter(|l| l.contains("Alice") && !l.contains("<Alice>"))
                    .collect::<Vec<_>>()
            );
        }
        let since_bob = bob.chat_len();
        say(&alice, "still ok", ChatFault::default(), "still ok").await;
        bob.wait_chat("still ok", since_bob, WAIT)
            .await
            .expect("still ok");
        logged(server, "<Alice> still ok", 1).await;
        // more than 64 signed messages were received since Bob last spoke: the bot acknowledged them
        assert!(
            bob.log_text().contains("-> play/chat_ack"),
            "no chat_ack was sent"
        );
        let mut out = section("alice", received(&alice));
        out.extend(section("bob", received(&bob)));
        out.extend(section("log", log_lines(server, "Alice")));
        out
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn a_corrupted_signature_is_refused_and_breaks_the_chain() {
    dual(async |mock, server| {
        let alice = chat_bot(server, mock, "Alice", true).await;
        say(&alice, "first", ChatFault::default(), "first").await;
        let corrupt = ChatFault {
            corrupt_signature: true,
            ..ChatFault::default()
        };
        say(&alice, "second", corrupt, "invalid").await;
        // the chain stays broken: even a correct message is refused
        say(&alice, "third", ChatFault::default(), "chain").await;
        logged(server, "Failed to update secure chat state for Alice", 2).await;
        let mut out = section("alice", received(&alice));
        out.extend(section("log", log_lines(server, "Alice")));
        out
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn a_message_older_than_the_previous_one_is_out_of_order() {
    dual(async |mock, server| {
        let alice = chat_bot(server, mock, "Alice", true).await;
        say(&alice, "first", ChatFault::default(), "first").await;
        let older = ChatFault {
            time_offset_ms: -60_000,
            ..ChatFault::default()
        };
        say(&alice, "second", older, "order").await;
        logged(server, "Failed to update secure chat state for Alice", 1).await;
        let mut out = section("alice", received(&alice));
        out.extend(section("log", log_lines(server, "Alice")));
        out
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn an_expired_message_is_logged_and_still_broadcast() {
    dual(async |mock, server| {
        let alice = chat_bot(server, mock, "Alice", true).await;
        let old = ChatFault {
            time_offset_ms: -600_000,
            ..ChatFault::default()
        };
        say(&alice, "old news", old, "old news").await;
        logged(server, "<Alice> old news", 1).await;
        let mut out = section("alice", received(&alice));
        out.extend(section("log", log_lines(server, "Alice").into_iter()));
        out
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and mc-server"]
async fn chat_without_a_chat_session_is_refused_when_the_profile_is_enforced() {
    dual(async |mock, server| {
        let alice = chat_bot(server, mock, "Alice", false).await;
        say(&alice, "unsigned", ChatFault::default(), "rofile").await;
        logged(server, "Failed to update secure chat state for Alice", 1).await;
        let mut out = section("alice", received(&alice));
        out.extend(section("log", log_lines(server, "Alice")));
        out
    })
    .await;
}
