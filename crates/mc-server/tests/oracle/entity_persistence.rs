//! Entities and riders survive a stop/start of the real server binary and a chunk unload: the same console script runs
//! against the vanilla server and `mc-server`, and the answers (counts, vehicle checks) must be the same.
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::common::*;
use mc_testbot::harness::vanilla;
use mc_testkit::Server;

static SYNC: AtomicU32 = AtomicU32::new(0);

/// Runs `command` and returns the log lines it produced: a following `say` marks the end (the console runs commands in
/// order on the server thread).
pub(crate) fn ask(server: &Server, command: &str) -> String {
    let before = server.log().lines().count();
    server.cmd(command);
    let marker = format!("sync-{}", SYNC.fetch_add(1, Ordering::Relaxed));
    server.cmd(&format!("say {marker}"));
    server
        .wait_log(&marker, 60)
        .unwrap_or_else(|| panic!("no answer to {command}\n{}", server.log()));
    server
        .log()
        .lines()
        .skip(before)
        .filter(|l| !l.contains(&marker))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn expect(server: &Server, command: &str, answer: &str) {
    let got = ask(server, command);
    assert!(
        got.contains(answer),
        "`{command}`: expected `{answer}` in\n{got}\n--- log\n{}",
        server.log()
    );
}

/// [`expect`] for an answer that needs ticks to arrive (chunks load and unload asynchronously).
pub(crate) fn eventually(server: &Server, command: &str, answer: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(40);
    loop {
        let got = ask(server, command);
        if got.contains(answer) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "`{command}`: expected `{answer}`, last answer\n{got}\n--- log\n{}",
            server.log()
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}

const CHUNK_LOADED: &str = "execute if loaded 8 100 8";
const KEPT: &str = "execute if entity @e[tag=kept]";

/// Summons the entities and checks them.
fn populate(server: &Server) {
    expect(server, "forceload add 0 0", "Marked chunk");
    eventually(server, CHUNK_LOADED, "Test passed");
    // frame support, then one of each kind (no gravity/AI: nothing moves, nothing despawns)
    expect(
        server,
        "setblock 8 100 8 minecraft:stone",
        "Changed the block",
    );
    for summon in [
        "summon minecraft:pig 8.5 100 2.5 {Tags:[\"kept\"],NoAI:1b,NoGravity:1b,PersistenceRequired:1b}",
        "summon minecraft:item 9.5 100 3.5 {Tags:[\"kept\"],NoGravity:1b,PickupDelay:32767s,Age:-32768s,Item:{id:\"minecraft:diamond\",count:3}}",
        "summon minecraft:item_frame 8 100 9 {Tags:[\"kept\"],Facing:3b,Item:{id:\"minecraft:apple\",count:1}}",
        "summon minecraft:minecart 2.5 100 12.5 {Tags:[\"kept\"],NoGravity:1b,Passengers:[{id:\"minecraft:chicken\",Tags:[\"kept\"],NoAI:1b,PersistenceRequired:1b,Silent:1b}]}",
        "summon minecraft:villager 12.5 100 12.5 {Tags:[\"kept\"],NoAI:1b,NoGravity:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:2}}",
    ] {
        expect(server, summon, "Summoned new");
    }
    eventually(server, KEPT, "Test passed. Count: 6");
    // a flushed save writes the dimension's saved data and the entity chunk while the server keeps running
    expect(server, "worldborder set 1000", "Set the world border");
    expect(server, "save-all flush", "Saved the game");
    let overworld = server.dir().join("world/dimensions/minecraft/overworld");
    for file in [
        "data/minecraft/world_border.dat",
        "data/minecraft/chunk_tickets.dat",
        "entities/r.0.0.mca",
    ] {
        assert!(
            overworld.join(file).is_file(),
            "{file} after save-all flush"
        );
    }
}

/// The chunk leaves the level and comes back, entities included.
fn unload_and_reload(server: &Server) {
    expect(server, "forceload remove 0 0", "Unmarked chunk");
    // the chunk drops a few ticks later, and its entities with it
    eventually(server, CHUNK_LOADED, "Test failed");
    expect(server, "forceload add 0 0", "Marked chunk");
    eventually(server, KEPT, "Test passed. Count: 6");
}

async fn run(server: &Server, restart: impl FnOnce() -> Server) {
    populate(server);
    unload_and_reload(server);

    // a rider: the boat carries a player, so it is saved with the player and not with its chunk
    expect(server, "op Bot", "operator");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    expect(
        server,
        "summon minecraft:oak_boat 0.5 100 0.5 {Tags:[\"rode\"],NoGravity:1b}",
        "Summoned new",
    );
    expect(
        server,
        "ride Bot mount @e[tag=rode,limit=1]",
        "started riding",
    );
    drop(bot);

    assert!(server.stop(), "stop\n{}", server.log());
    let server = restart();
    expect(&server, "forceload add 0 0", "hunk");
    eventually(&server, KEPT, "Test passed. Count: 6");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    expect(
        &server,
        "execute as Bot on vehicle if entity @s[type=minecraft:oak_boat,tag=rode]",
        "Test passed",
    );
    drop(bot);
    assert!(server.stop(), "second stop\n{}", server.log());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn vanilla_entities_and_riders_survive_a_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let start = || {
        vanilla(12345, "creative", &[])
            .dir(dir.path())
            .start()
            .expect("vanilla")
    };
    run(&start(), start).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn our_entities_and_riders_survive_a_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let start = || {
        ours(12345, "creative", &[])
            .dir(dir.path())
            .start()
            .expect("ours")
    };
    run(&start(), start).await;
}
