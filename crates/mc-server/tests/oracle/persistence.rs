//! Restart persistence (position, inventory, world change survive a restart with the bot still connected to the
//! stopping server) and world-file compatibility in both directions: the vanilla server opens the world our server
//! saved, and our server opens the world the vanilla server saved.
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use crate::common::*;
use mc_core::pos::block_pos::BlockPos;
use mc_data::{Block, Item};
use mc_protocol::components::ItemStack;
use mc_testbot::Bot;
use mc_testbot::harness::vanilla;

struct Saved {
    pos: (f64, f64, f64),
    block: BlockPos,
}

/// Op the bot, give it a stack, walk, place a block with `setblock`; returns what must persist.
async fn mutate(bot: &Arc<Bot>, console: &(dyn Console + Sync)) -> Saved {
    console.cmd("op Bot");
    console.wait_log("operator", 20).expect("op feedback");
    bot.creative_set_slot(36, ItemStack::with_count(Item::STONE, 64));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let start = bot.position();
    let _ = bot
        .walk_to(start.x + 5.0, start.z, 0.3, false, Duration::from_secs(10))
        .await;
    bot.stop();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let p = bot.position();
    let block = BlockPos::new(
        p.x.floor() as i32 + 2,
        p.y.floor() as i32 + 4,
        p.z.floor() as i32,
    );
    let fb = bot
        .command_expect(
            &format!(
                "setblock {} {} {} minecraft:diamond_block",
                block.x(),
                block.y(),
                block.z()
            ),
            "Changed the block",
            T,
        )
        .await
        .unwrap_or_else(|e| {
            panic!(
                "setblock: {e}\n{}",
                bot.log_text()
                    .lines()
                    .filter(|l| l.contains("chat")
                        || l.contains("teleport")
                        || l.contains("position"))
                    .rev()
                    .take(15)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
    println!("setblock -> {fb}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let p = bot.position();
    Saved {
        pos: (p.x, p.y, p.z),
        block,
    }
}

async fn verify(port: u16, saved: &Saved, what: &str) {
    let bot = join("127.0.0.1", port, "Bot").await;
    let p = bot.position();
    assert!(
        (p.x - saved.pos.0).abs() < 0.2
            && (p.y - saved.pos.1).abs() < 0.2
            && (p.z - saved.pos.2).abs() < 0.2,
        "{what}: position {p:?} != saved {:?}",
        saved.pos
    );
    assert!(
        bot.slot(36)
            .is_some_and(|s| s.is(Item::STONE) && s.count() == 64),
        "{what}: inventory slot 36 = {:?}",
        bot.slot(36)
    );
    // the chunk with the placed block arrives right after join
    let found = bot
        .wait_for(Duration::from_secs(30), |s| {
            s.world
                .block_state_at(saved.block)
                .filter(|b| b.block() == Block::DIAMOND_BLOCK)
                .map(|_| ())
        })
        .await;
    assert!(
        found.is_ok(),
        "{what}: block at {:?} is {:?}",
        saved.block,
        bot.block_at(saved.block)
    );
    bot.disconnect().await;
    println!("PERSIST OK {what}");
}

fn copy_world(from: &Path, to: &Path) {
    let _ = std::fs::remove_dir_all(to.join("world"));
    std::fs::create_dir_all(to).unwrap();
    let st = Command::new("cp")
        .arg("-r")
        .arg(from.join("world"))
        .arg(to.join("world"))
        .status()
        .expect("cp");
    assert!(st.success());
}

/// Our server on `dir` (a restart reuses the directory and gets a new port).
fn start_ours(dir: &Path) -> mc_testkit::Server {
    ours(12345, "creative", &[])
        .dir(dir)
        .start()
        .expect("start ours")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn restart_and_cross_server_world_compat() {
    // 1. restart persistence, bot still connected when `stop` arrives
    let ours_dir = tempfile::tempdir().expect("temp dir");
    let server = start_ours(ours_dir.path());
    let bot = join("127.0.0.1", server.port, "Bot").await;
    let saved = mutate(&bot, &server).await;
    assert!(
        server.stop(),
        "our server did not stop within 60 s with a player online\n{}",
        server.log()
    );
    assert!(server.log().contains("Stopping server"), "{}", server.log());
    drop(bot);
    let server = start_ours(ours_dir.path());
    verify(server.port, &saved, "ours -> ours").await;
    assert!(server.stop(), "stop after rejoin");

    // 2. vanilla opens the world our server saved
    let tmp_a = tempfile::tempdir().expect("temp dir");
    let van = vanilla(12345, "creative", &[])
        .dir(tmp_a.path())
        .start()
        .expect("vanilla");
    van.stop();
    copy_world(ours_dir.path(), tmp_a.path());
    let van = vanilla(12345, "creative", &[])
        .dir(tmp_a.path())
        .start()
        .expect("vanilla on our world");
    verify(van.port, &saved, "ours -> vanilla").await;
    van.stop();

    // 3. our server opens the world the vanilla server saved
    let tmp_b = tempfile::tempdir().expect("temp dir");
    let van = vanilla(12345, "creative", &[])
        .dir(tmp_b.path())
        .start()
        .expect("vanilla");
    let bot = join("127.0.0.1", van.port, "Bot").await;
    let saved = mutate(&bot, &van).await;
    drop(bot);
    van.stop();
    assert!(tmp_b.path().join("world/level.dat").exists());
    copy_world(van.dir(), ours_dir.path());
    let server = start_ours(ours_dir.path());
    verify(server.port, &saved, "vanilla -> ours").await;
}
