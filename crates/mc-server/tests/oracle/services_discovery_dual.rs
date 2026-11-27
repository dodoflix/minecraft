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
    records_within(server, expected, 30)
}

fn records_within(server: &Server, expected: usize, seconds: u64) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
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
fn check(url: &str, expected: usize) -> Result<(), String> {
    let vanilla = start(harness::vanilla(7, "survival", &[]), url);
    let theirs = records(&vanilla, expected);
    vanilla.stop();
    drop(vanilla);
    let ours = start(ours(7, "survival", &[]), url);
    let mine = records(&ours, expected);
    println!("== {url}\n{}", theirs.join("\n"));
    if theirs.len() != expected {
        return Err(format!("vanilla logged\n{}", theirs.join("\n")));
    }
    if theirs != mine {
        return Err(format!(
            "records: vanilla\n{}\nours\n{}",
            theirs.join("\n"),
            mine.join("\n")
        ));
    }
    Ok(())
}

fn dual(url: &str, expected: usize) {
    check(url, expected).unwrap_or_else(|e| panic!("{e}"));
}

/// [`dual`] for each body the service at `path` answers with `status`; every difference is reported, not the first.
fn dual_each(mock: &MockMojang, path: &str, status: u16, bodies: Vec<serde_json::Value>) {
    let mut failures = Vec::new();
    for body in bodies {
        println!("== {body}");
        mock.respond(path, status, body.clone());
        if let Err(e) = check(&mock.discovery_url(), 2) {
            failures.push(format!("{body}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
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
fn key_sets_of_every_shape_log_alike() {
    let mock = MockMojang::start().expect("start the mock");
    let key = "AAAA";
    let bodies = vec![
        json!({"playerCertificateKeys": "keys"}),
        json!({"playerCertificateKeys": {"publicKey": key}}),
        json!({"playerCertificateKeys": [5]}),
        json!({"playerCertificateKeys": ["keys"]}),
        json!({"playerCertificateKeys": [[]]}),
        json!({"playerCertificateKeys": [{"publicKey": "!!!"}]}),
        json!({"playerCertificateKeys": [{"publicKey": "AAA"}]}),
        json!({"playerCertificateKeys": [{"publicKey": 12345}]}),
        json!({"playerCertificateKeys": [{"publicKey": true}]}),
        json!({"playerCertificateKeys": [{"publicKey": {}}]}),
        json!({"playerCertificateKeys": [{"publicKey": []}]}),
        json!({"playerCertificateKeys": [{"publicKey": null}]}),
        json!({"playerCertificateKeys": [{}]}),
        json!({"playerCertificateKeys": [null]}),
        json!({"profilePropertyKeys": [{"publicKey": key}]}),
        json!({"profilePropertyKeys": "keys"}),
        json!({"profilePropertyKeys": [{"publicKey": "!!!"}], "playerCertificateKeys": []}),
        json!({"playerCertificateKeys": [{"publicKey": key, "other": 1}, {"publicKey": key}]}),
        json!([]),
        json!("keys"),
    ];
    dual_each(&mock, "/publickeys", 200, bodies);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn error_bodies_of_every_type_log_alike() {
    let mock = MockMojang::start().expect("start the mock");
    let bodies = vec![
        json!({"path": 5, "error": true, "errorMessage": 1.5}),
        json!({"path": 100000000, "error": 12345678901234567890u64, "errorMessage": 0.0000001}),
        json!({"path": {}}),
        json!({"error": []}),
        json!({"errorMessage": {"a": 1}}),
        json!({"details": "text"}),
        json!({"details": [1]}),
        json!({"details": []}),
        json!({"details": [["a", 1]]}),
        json!({"details": [["a", 1], ["b", [2, "x", null]], [7, {"c": true}], [true, null]]}),
        json!({"details": [["a"]]}),
        json!({"details": [["a", 1, 2]]}),
        json!({"details": [["a", 1], ["a", 2]]}),
        json!({"details": [["a", null], ["a", 2], ["b", 3]]}),
        json!({"details": [[[], 1]]}),
        json!({"details": [[null, 1]]}),
        json!({"path": {}, "details": [[null, 1]]}),
        json!({"details": [[null, 1]], "path": {}}),
        json!({"details": null, "path": null}),
        json!({"details": {"a": [1, {"b": null, "c": true}], "d": 1e10, "e": 0.0001, "f": 1.5e-7, "g": "x"}}),
        json!({"details": {"n": 100, "m": 12345678.9, "k": 1e7, "j": 9999999.0, "i": 1e21}}),
        json!({"errorMessage": "multi\nline", "path": "tab\t"}),
        json!([1, 2]),
        json!(7),
        json!(true),
    ];
    dual_each(&mock, "/publickeys", 500, bodies);
}

/// Both servers at once against `url`: `first` records after the first attempt, `after` once the retry five minutes later
/// has run too.
fn retried(url: &str, first: usize, after: usize) {
    let vanilla = start(harness::vanilla(7, "survival", &[]), url);
    let ours = start(ours(7, "survival", &[]), url);
    let (theirs, mine) = (records(&vanilla, first), records(&ours, first));
    assert_eq!(theirs.len(), first, "vanilla logged\n{}", theirs.join("\n"));
    assert_eq!(
        theirs, mine,
        "first attempt: vanilla (left) vs ours (right)"
    );
    let (theirs, mine) = (
        records_within(&vanilla, after, 330),
        records_within(&ours, after, 330),
    );
    println!("== {url} retried\n{}", theirs.join("\n"));
    assert_eq!(theirs.len(), after, "vanilla logged\n{}", theirs.join("\n"));
    assert_eq!(theirs, mine, "retry: vanilla (left) vs ours (right)");
}

#[test]
#[ignore = "oracle: five minutes, starts the vanilla server and the mc-server binary"]
fn a_failed_key_fetch_is_retried_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond("/publickeys", 500, json!({"errorMessage": "nope"}));
    retried(&mock.discovery_url(), 2, 4);
}

#[test]
#[ignore = "oracle: five minutes, starts the vanilla server and the mc-server binary"]
fn a_failed_discovery_fetch_is_retried_alike() {
    let mock = MockMojang::start().expect("start the mock");
    mock.respond("/minecraft/client", 500, json!({"errorMessage": "down"}));
    // the document's fetch again after 1 and 3 minutes, then both key sets after 5
    retried(&mock.discovery_url(), 5, 11);
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
