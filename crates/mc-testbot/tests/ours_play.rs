//! The same acceptance scenarios against OUR server. Needs the `mc-server` binary (`MC_SERVER_BIN` or
//! `target/*/debug/mc-server`); skipped otherwise.
mod common;
use common::*;
use mc_testbot::harness::OurServer;

impl Console for OurServer {
    fn cmd(&self, command: &str) {
        OurServer::cmd(self, command)
    }
    fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        OurServer::wait_log(self, regex, timeout_s)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bot_plays_on_our_server() {
    let Some(bin) = OurServer::binary() else {
        eprintln!("skipping: no mc-server binary");
        return;
    };
    let server = OurServer::start(&bin, 12345, "creative").expect("start our server");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    server.cmd("op Bot");
    server.wait_log("operator", 20);

    let failed = run_suite(&bot, &server).await;
    let _ = bot.write_log(mc_testbot::harness::repo_root().join("run/testbot-ours.jsonl"));
    bot.disconnect().await;
    assert!(failed.is_empty(), "failed scenarios: {failed:?}\nserver log tail:\n{}", server.log().lines().rev().take(30).collect::<Vec<_>>().join("\n"));
}
