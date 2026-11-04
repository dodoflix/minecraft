//! Acceptance scenarios against the real vanilla server (offline, fixed seed).
mod common;
use common::*;
use mc_testbot::harness::{vanilla_available, VanillaServer};
use std::time::Duration;

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
    let _ = bot.write_log(mc_testbot::harness::repo_root().join("run/testbot-vanilla.jsonl"));

    walk_scenario(&bot, 32).await;
    chat_and_command(&bot).await;
    place_and_break(&bot, true).await;
    inventory_move(&bot, "Bot").await;

    // survival: break and place with real mining time
    server.cmd("gamemode survival Bot");
    server.cmd("give Bot minecraft:dirt 16");
    bot.wait_for(Duration::from_secs(20), |s| (!s.body.abilities.instabuild && !s.held_item().is_empty()).then_some(())).await
        .unwrap_or_else(|e| panic!("survival switch/give: {e}\n{}", bot.log_tail(40)));
    place_and_break(&bot, false).await;
    bot.assert_connected().unwrap();
    let _ = bot.write_log(mc_testbot::harness::repo_root().join("run/testbot-vanilla.jsonl"));
    bot.disconnect().await;
}
