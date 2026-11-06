//! The same acceptance scenarios against OUR server.
use crate::common::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn bot_plays_on_our_server() {
    let server = ours(12345, "creative", &[])
        .start()
        .expect("start our server");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("operator", 20);

    let failed = run_suite(&bot, &server).await;
    let _ = bot.write_log(mc_testbot::harness::repo_root().join("run/testbot-ours.jsonl"));
    bot.disconnect().await;
    assert!(
        failed.is_empty(),
        "failed scenarios: {failed:?}\nserver log tail:\n{}",
        server
            .log()
            .lines()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
