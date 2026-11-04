use std::sync::{Arc, Mutex};

use mc_testbot::harness::{vanilla_available, VanillaServer};
use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

#[tokio::test(flavor = "multi_thread")]
async fn status_and_configuration_against_vanilla() {
    if !vanilla_available() {
        eprintln!("skipping: vanilla oracle unavailable");
        return;
    }
    let server = VanillaServer::start("tb-join", 12345, "creative", &[]).expect("start vanilla");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let ping = status_ping(&opts, log.clone()).await.expect("ping");
    assert_eq!(ping.status.version.as_ref().unwrap().protocol, 777);

    let mut conn = open(&opts, log.clone()).await.unwrap();
    let cfg = login_and_configure(&mut conn, &opts).await.expect("login+configure");
    assert_eq!(cfg.profile_name, "Bot");
    assert!(cfg.registries.len() > 20, "{} registries", cfg.registries.len());
    assert!(!cfg.tags.is_empty());
    assert_eq!(cfg.server_brand.as_deref(), Some("vanilla"));
    println!("{}", log.lock().unwrap().to_text());
}
