//! `management-server-enabled=true` with a secret that is not 40 alphanumeric characters: the server refuses to start
//! before it opens the world. The startup output, the exit and the absence of a world folder equal vanilla's; with a
//! valid secret the `Starting json RPC server on` line still comes before the world loads.
use mc_testkit::Builder;
use std::time::Duration;

/// The startup lines of a server that is expected to refuse: timestamps and stack traces left out.
fn refused(builder: Builder) -> (Vec<String>, bool) {
    let server = builder
        .property("management-server-enabled", "true")
        .property("management-server-secret", &"a".repeat(39))
        .start_unready()
        .expect("launch");
    let exited = server.wait_exit(Duration::from_secs(60)).is_some();
    let lines: Vec<String> = server
        .log()
        .lines()
        .filter(|l| l.starts_with('['))
        .filter_map(|l| l.split_once("] ").map(|(_, rest)| rest.to_owned()))
        .filter(|l| !l.contains("Time elapsed") && !l.contains("Environment:"))
        .collect();
    let world = server.dir().join("world").exists();
    let mut out = lines;
    out.push(format!("world folder created: {world}"));
    (out, exited)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn a_bad_management_secret_stops_before_the_world_opens() {
    let (vanilla, vanilla_exited) = refused(mc_testbot::harness::vanilla(7, "survival", &[]));
    let (ours, ours_exited) = refused(crate::common::ours(7, "survival", &[]));
    println!("{}", vanilla.join("\n"));
    assert!(
        vanilla_exited && ours_exited,
        "both servers exit by themselves"
    );
    assert_eq!(vanilla, ours, "vanilla (left) vs ours (right)");
}
