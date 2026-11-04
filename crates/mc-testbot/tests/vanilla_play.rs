//! Acceptance scenarios against the real vanilla server (offline, fixed seed).
mod common;
use common::*;
use mc_testbot::harness::{vanilla_available, VanillaServer};

#[tokio::test(flavor = "multi_thread")]
async fn bot_plays_on_vanilla() {
    if !vanilla_available() {
        eprintln!("skipping: vanilla oracle unavailable");
        return;
    }
    let server = VanillaServer::start("tb-play", 12345, "creative", &[]).expect("start vanilla");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("Made Bot a server operator", 20);
    let failed = run_suite(&bot, &server).await;
    let _ = bot.write_log(mc_testbot::harness::repo_root().join("run/testbot-vanilla.jsonl"));
    bot.disconnect().await;
    assert!(failed.is_empty(), "failed scenarios: {failed:?}");
}
