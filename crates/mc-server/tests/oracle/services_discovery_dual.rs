//! The Mojang Services discovery thread's log records at server start, on the real vanilla server and on `mc-server`,
//! with the services pointed at an address nothing listens on and at the mock (`mc_testkit::MockMojang`) made to fail in
//! several ways. Vanilla starts one fetch of the discovery document and two key set fetches on that thread; each failure
//! is a log record with its message and the first line of its exception (stack frames and `Caused by` blocks are Java's).
use crate::common::ours;
use crate::server_properties_dual::{mask_ports, message};
use mc_testbot::harness;
use mc_testkit::{Builder, MockMojang, Server};
use serde_json::json;
use std::time::{Duration, Instant};

const THREAD: &str = "Minecraft Services Discovery/";

/// The thread's records in `log` as `LEVEL: message` followed by the exception's first line when there is one.
fn parse(log: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_record = false;
    for line in log.lines() {
        if let Some(text) = message(line) {
            in_record = text.starts_with(THREAD);
            if in_record {
                out.push(mask_ports(text.trim_start_matches(THREAD)));
            }
        } else if in_record
            && !line.starts_with(['\t', ' '])
            && !line.starts_with("Caused by:")
            && !out.last().is_some_and(|r| r.contains('\n'))
        {
            let last = out.last_mut().expect("a record");
            last.push('\n');
            last.push_str(&mask_ports(line));
        }
    }
    out
}

/// The thread's records once `expected` are in the log.
fn records(server: &Server, expected: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while parse(&server.log()).len() < expected && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    // a record's exception line is a line of its own that may still be on its way
    std::thread::sleep(Duration::from_millis(500));
    parse(&server.log())
}

fn start(builder: Builder, url: &str) -> Server {
    builder
        .discovery_url(url)
        .start()
        .expect("start the server")
}

/// Vanilla first, then ours, against the same discovery document; both must log the same records.
fn dual(url: &str, expected: usize) {
    let vanilla = start(harness::vanilla(7, "survival", &[]), url);
    let theirs = records(&vanilla, expected);
    vanilla.stop();
    drop(vanilla);
    let ours = start(ours(7, "survival", &[]), url);
    let mine = records(&ours, expected);
    println!("== {url}\n{}", theirs.join("\n"));
    assert_eq!(
        theirs.len(),
        expected,
        "vanilla logged\n{}",
        theirs.join("\n")
    );
    assert_eq!(theirs, mine, "records: vanilla (left) vs ours (right)");
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn an_unreachable_discovery_service_logs_alike() {
    let url = format!(
        "http://127.0.0.1:{}/minecraft/client",
        mc_testkit::free_port()
    );
    dual(&url, 5);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn a_discovery_service_that_answers_an_error_logs_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond("/minecraft/client", 500, json!({"errorMessage": "down"}));
    dual(&mock.discovery_url(), 5);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn an_error_body_with_details_and_one_that_is_no_object_log_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond(
        "/minecraft/client",
        503,
        json!({"path": "/minecraft/client", "error": "Busy", "errorMessage": "later",
               "details": {"retry": 5, "reason": "load"}}),
    );
    dual(&mock.discovery_url(), 5);
    mock.respond("/minecraft/client", 502, json!("Bad Gateway"));
    dual(&mock.discovery_url(), 5);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn a_discovery_document_without_the_key_endpoint_logs_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond(
        "/minecraft/client",
        200,
        json!({"environment": "mock", "product": "minecraft",
               "discovery": {"authentication": {"endpoints": {}}}}),
    );
    dual(&mock.discovery_url(), 4);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn a_key_service_that_answers_an_error_logs_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond("/publickeys", 500, json!({"errorMessage": "nope"}));
    dual(&mock.discovery_url(), 2);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn malformed_service_keys_log_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond(
        "/publickeys",
        200,
        json!({"playerCertificateKeys": [{"publicKey": "AAAA"}]}),
    );
    dual(&mock.discovery_url(), 2);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn a_working_service_logs_nothing() {
    for vanilla in [true, false] {
        let mock = MockMojang::start().expect("start the mock");
        let builder = if vanilla {
            harness::vanilla(7, "survival", &[])
        } else {
            ours(7, "survival", &[])
        };
        let server = start(builder, &mock.discovery_url());
        // both servers fetch the key set twice
        let deadline = Instant::now() + Duration::from_secs(20);
        let fetches = || {
            mock.requests()
                .iter()
                .filter(|r| r.target == "/publickeys")
                .count()
        };
        while fetches() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(fetches(), 2, "vanilla: {vanilla}");
        assert!(records(&server, 0).is_empty(), "{}", server.log());
    }
}
