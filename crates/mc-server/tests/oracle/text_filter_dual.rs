//! The text filter (version 0) against a mock moderation service, on the real vanilla server and on `mc-server`:
//! what each server sends (method, path, headers, body of the chat, join and leave requests), what the players see
//! (the filter mask and unsigned content of `player_chat`, the book the filter touched) and what the server logs
//! (the warnings, with the first line of the exception) for the answers a service can give, and the startup lines
//! for a configuration that cannot be used.
use mc_protocol::packets::play::*;
use mc_testbot::join::BotOptions;
use mc_testbot::log::Dir;
use mc_testbot::{Bot, harness};
use mc_testkit::{FilterAnswer, FilterRequest, MockFilterServer, Server};
use std::sync::Arc;
use std::time::Duration;

fn config(url: &str, extra: &str) -> String {
    format!(r#"{{"apiServer":"{url}","apiKey":"k3y"{extra}}}"#)
}

async fn connect(port: u16, name: &str, filtering: bool) -> Arc<Bot> {
    let mut o = BotOptions::new("127.0.0.1", port, name);
    o.timeout = Duration::from_secs(120);
    o.record_raw = true;
    o.client_information.text_filtering_enabled = filtering;
    let bot = Bot::connect(o).await.expect("connect");
    bot.wait_joined(Duration::from_secs(60))
        .await
        .expect("login packet");
    bot.wait_ready(Duration::from_secs(90))
        .await
        .expect("player loaded");
    Arc::new(bot)
}

fn from_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The `player_chat` packets a bot received, with the parts the filter changes.
fn chat_packets(bot: &Bot) -> Vec<String> {
    let log = bot.shared.log.lock().unwrap();
    let mut out = Vec::new();
    for e in log.entries() {
        if e.dir != Dir::In || e.state != "play" || e.name != "player_chat" {
            continue;
        }
        match ClientboundPlayPacket::decode_payload(&from_hex(e.raw.as_ref().unwrap())) {
            Ok(ClientboundPlayPacket::PlayerChat(p)) => out.push(format!(
                "player_chat {:?} unsigned={:?} mask={:?}",
                p.body.content, p.unsigned_content, p.filter_mask
            )),
            other => out.push(format!("undecodable player_chat: {:?}", other.err())),
        }
    }
    out
}

/// The log lines of the filter: timestamps cut, every warning with the first line of its exception.
fn filter_log(server: &Server) -> Vec<String> {
    let log = server.log();
    let lines: Vec<&str> = log.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let interesting = line.contains("Chat-Filter-Worker")
            || line.contains("Failed to parse chat filter config")
            || line.contains("text filter");
        if !interesting {
            continue;
        }
        out.push(
            line.split_once("] ")
                .map_or(*line, |(_, rest)| rest)
                .to_owned(),
        );
        if let Some(next) = lines.get(i + 1)
            && !next.starts_with('[')
        {
            out.push((*next).to_owned());
        }
    }
    out
}

/// The requests the mock saw; the `User-Agent` the JDK picks for a redirected GET names its own version.
fn requests(mock: &MockFilterServer) -> Vec<String> {
    mock.requests()
        .iter()
        .map(|r| {
            r.summary()
                .lines()
                .map(|l| {
                    if l.starts_with("user-agent: Java/") {
                        "user-agent: Java/<version>"
                    } else {
                        l
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// The answer of the mock to the chat message `text`.
fn answer_for(request: &FilterRequest) -> FilterAnswer {
    if request.target != "/v1/chat" && request.target != "/v2/text" {
        return FilterAnswer::Json(200, "{}".into());
    }
    let json = |s: &str| FilterAnswer::Json(200, s.to_owned());
    let text = request.body.split(r#""text":""#).nth(1).unwrap_or("");
    let text = text.split('"').next().unwrap_or("");
    match text {
        "hashed" => json(r#"{"response":false,"hashed":"h##o","hashes":[1,2]}"#),
        "nohash" => json(r#"{"response":false}"#),
        "nullhash" => json(r#"{"response":false,"hashed":null}"#),
        "emptyhashes" => json(r#"{"response":false,"hashed":"x","hashes":[]}"#),
        "negative" => json(r#"{"response":false,"hashed":"x","hashes":[-1]}"#),
        "beyond" => json(r#"{"response":false,"hashed":"x","hashes":[40,0]}"#),
        "allhashed" => json(r#"{"response":false,"hashed":"x","hashes":[0,1,2,3,4,5,6,7,8]}"#),
        "n204" => FilterAnswer::Status(204),
        "e500" => FilterAnswer::Status(500),
        "r302" => FilterAnswer::Redirect("/v1/redirected".into()),
        "badjson" => json("not json"),
        "array" => json("[1]"),
        "slow" => FilterAnswer::Slow(Duration::from_secs(3), r#"{"response":true}"#.into()),
        _ => json(r#"{"response":true}"#),
    }
}

/// One server (vanilla or ours) with a text filter pointed at `mock`: two observers (filter on, off) and a sender
/// who says each message and edits a writable book through the same filter.
async fn session(server: &Server, mock: &MockFilterServer, messages: &[&str]) -> Vec<String> {
    let sender = connect(server.port, "Sender", false).await;
    let on = connect(server.port, "ObsOn", true).await;
    let off = connect(server.port, "ObsOff", false).await;
    server.cmd("gamemode creative Sender");
    for message in messages {
        sender.chat(message);
        // the request, the answer of the worker, the game thread's broadcast (the slow answer outlasts the timeout)
        let settle = if *message == "slow" { 5000 } else { 1200 };
        tokio::time::sleep(Duration::from_millis(settle)).await;
    }
    sender.creative_set_slot(
        36,
        mc_protocol::components::ItemStack::with_count(mc_data::Item::WRITABLE_BOOK, 1),
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    sender.send(ServerboundPlayPacket::EditBook(ServerboundEditBookPacket {
        slot: 0,
        pages: vec!["hashed".into(), "pass".into()],
        title: Some("hashed".into()),
    }));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    server.cmd("data get entity Sender Inventory");
    server
        .wait_log(r"Sender has the following entity data: \[", 30)
        .expect("inventory dump");
    let mut out = Vec::new();
    out.push("-- sender hears".to_owned());
    out.extend(chat_packets(&sender));
    out.push("-- observer with the filter on".to_owned());
    out.extend(chat_packets(&on));
    out.push("-- observer with the filter off".to_owned());
    out.extend(chat_packets(&off));
    let log = server.log();
    let dump = log
        .lines()
        .find(|l| l.contains("Sender has the following entity data"))
        .and_then(|l| l.split_once("entity data: "))
        .map(|(_, d)| d.to_owned())
        .unwrap_or_default();
    out.push(format!("-- inventory {dump}"));
    sender.disconnect().await;
    on.disconnect().await;
    off.disconnect().await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    out.push("-- requests".to_owned());
    out.extend(requests(mock));
    out.push("-- log".to_owned());
    out.extend(filter_log(server));
    out
}

async fn run_both(extra: &str, messages: &[&str]) {
    let mut sides = Vec::new();
    for ours in [false, true] {
        let mock = MockFilterServer::start().expect("mock filter server");
        mock.respond_with(answer_for);
        let prop = format!("text-filtering-config={}", config(&mock.url(), extra));
        let props = [prop.as_str()];
        let builder = if ours {
            crate::common::ours(7, "survival", &props)
        } else {
            harness::vanilla(7, "survival", &props)
        };
        let server = builder.start().expect("start server");
        let rows = session(&server, &mock, messages).await;
        server.stop();
        sides.push(rows);
    }
    let [vanilla, ours] = &sides[..] else {
        unreachable!()
    };
    println!("{}", vanilla.join("\n"));
    assert_eq!(vanilla, ours, "vanilla (left) vs ours (right)");
}

const MESSAGES: [&str; 14] = [
    "pass",
    "hashed",
    "nohash",
    "nullhash",
    "emptyhashes",
    "negative",
    "beyond",
    "allhashed",
    "n204",
    "e500",
    "r302",
    "badjson",
    "array",
    "slow",
];

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn answers_never_ignored() {
    run_both("", &MESSAGES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn answers_with_zero_hashes_to_drop() {
    run_both(
        r#","hashesToDrop":0"#,
        &["hashed", "allhashed", "emptyhashes"],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn answers_with_a_threshold_of_two_hashes() {
    run_both(r#","hashesToDrop":2"#, &["hashed", "beyond", "emptyhashes"]).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn another_chat_endpoint_and_rule_id() {
    run_both(
        r#","ruleId":3,"serverId":"srv","roomId":"room","endpoints":{"chat":"v2/text","join":"in","leave":"out"}"#,
        &["pass", "hashed"],
    )
    .await;
}

/// A configuration that does not make a filter: the warning, with the first line of the exception, at startup.
fn config_rows(config: &str, version: i32) -> Vec<String> {
    let mut sides = Vec::new();
    for ours in [false, true] {
        let a = format!("text-filtering-config={config}");
        let b = format!("text-filtering-version={version}");
        let props = [a.as_str(), b.as_str()];
        let builder = if ours {
            crate::common::ours(7, "survival", &props)
        } else {
            harness::vanilla(7, "survival", &props)
        };
        let server = builder.start().expect("start server");
        let rows = filter_log(&server);
        server.stop();
        sides.push(rows);
    }
    let [vanilla, ours] = &sides[..] else {
        unreachable!()
    };
    assert_eq!(vanilla, ours, "vanilla (left) vs ours (right) for {config}");
    vanilla.clone()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn unusable_configurations_log_like_vanilla() {
    for (config, version) in [
        (r#"{"apiServer":"http://127.0.0.1:9"}"#, 0),
        (r#"{"apiServer":"http://127.0.0.1:9","apiKey":""}"#, 0),
        (r#"{"apiKey":"k"}"#, 0),
        (
            r#"{"apiServer":"http://127.0.0.1:9","apiKey":"k","hashesToDrop":"x"}"#,
            0,
        ),
        (r#"not json"#, 0),
        (r#"[]"#, 0),
        (r#"{"apiServer":"http://127.0.0.1:9","apiKey":"k"}"#, 7),
    ] {
        let rows = config_rows(config, version);
        println!("{config} v{version}: {rows:?}");
        assert!(!rows.is_empty(), "no log line for {config}");
    }
}
