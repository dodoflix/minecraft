//! Player `.dat` parity: the same scenario runs against the real vanilla server and ours (offline "Bot", survival,
//! same seed), the saved `world/players/data/<uuid>.dat` files are diffed key by key (paths, tag types, values, key
//! order, list order). The populated case starts both servers from the same hand-written player file instead of a
//! fresh player.
use std::path::Path;
use std::time::Duration;

use crate::common::world_diff::{Rules, diff_compound, sort_unordered};
use crate::common::*;
use mc_nbt::{CompoundTag, ListTag, snbt};
use mc_testbot::harness::vanilla;

fn player_file(dir: &Path) -> std::path::PathBuf {
    dir.join("world/players/data")
}

fn read_dat(dir: &Path) -> CompoundTag {
    let data = player_file(dir);
    let file = std::fs::read_dir(&data)
        .unwrap_or_else(|e| panic!("{}: {e}", data.display()))
        .flatten()
        .find(|e| e.path().extension().is_some_and(|x| x == "dat"))
        .expect("player file")
        .path();
    let bytes = std::fs::read(file).expect("read dat");
    mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap())
        .expect("nbt")
}

/// How far two numbers of the two servers may be apart and still count as equal.
#[derive(Clone, Copy)]
struct Slack {
    /// The position of a fresh player: `PlayerSpawnFinder` offsets it inside the respawn radius with
    /// `RandomSource.createThreadLocalInstance()`, so two positions around the same spawn are at most two radii apart;
    /// a walking bot adds the distance it walks.
    pos: f64,
    /// Exhaustion and saturation: a walk of wall-clock length spends food, so the walked case ignores them.
    food: f64,
}

/// How far two numbers may be apart at `path` and still count as equal: the values the clock of the run drives (the
/// food level regrows on peaceful, effect durations, the warden tracker) and `slack`.
fn tolerance(path: &str, slack: Slack) -> f64 {
    match path {
        p if p.starts_with(".Pos[") => slack.pos,
        ".foodSaturationLevel" | ".foodExhaustionLevel" => slack.food,
        ".foodLevel" => 4.0,
        // ticks of the save-all latency and the wall-clock walk (20 s)
        ".warden_spawn_tracker.ticks_since_last_warning" => 400.0,
        p if p.starts_with(".active_effects[") && p.ends_with(".duration") => 400.0,
        _ => 0.0,
    }
}

async fn scenario(console: &(dyn Console + Sync), port: u16, walk: bool) {
    let bot = join("127.0.0.1", port, "Bot").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    if walk {
        let start = bot.position();
        let _ = bot
            .walk_to(start.x + 6.0, start.z, 0.4, true, Duration::from_secs(12))
            .await;
        bot.stop();
        tokio::time::sleep(Duration::from_secs(8)).await;
    }
    console.cmd("save-all flush");
    console.wait_log("Saved the game", 60).expect("saved");
    bot.disconnect().await;
}

/// Writes `seed` as the player file of "Bot" (the offline uuid) before the first launch.
fn write_seed(dir: &Path, seed: &CompoundTag) {
    let uuid = mc_core::uuid_util::create_offline_player_uuid("Bot");
    let data = player_file(dir);
    std::fs::create_dir_all(&data).expect("create the player directory");
    std::fs::write(
        data.join(format!("{uuid}.dat")),
        mc_nbt::compress::write_compressed(seed),
    )
    .expect("write the seeded player file");
}

async fn run_both(walk: bool, seed: Option<CompoundTag>) -> (CompoundTag, CompoundTag) {
    let prepared = |b: mc_testkit::Builder| match seed.clone() {
        Some(seed) => b.prepare(move |dir| write_seed(dir, &seed)),
        None => b,
    };
    let van = prepared(vanilla(12345, "survival", &[]))
        .start()
        .expect("vanilla");
    scenario(&van, van.port, walk).await;
    van.stop();
    let vanilla = read_dat(van.dir());
    let ours = prepared(ours(12345, "survival", &[]))
        .start()
        .expect("ours");
    scenario(&ours, ours.port, walk).await;
    assert!(ours.stop());
    let o = read_dat(ours.dir());
    (vanilla, o)
}

/// A player with everything the file can hold: inventory with components, armor, effects, experience, the ender chest,
/// abilities, the respawn point, the last death location, the nether entry position, the credits flag, the warden
/// tracker, the recipe book and a shoulder parrot. `pos` is a standing place the fresh player was put on.
fn populated(pos: &ListTag) -> CompoundTag {
    let mut seed = snbt::parse_compound(
        r#"{
            Inventory:[
                {Slot:0b,id:"minecraft:diamond_sword",count:1,components:{"minecraft:enchantments":{"minecraft:sharpness":3},"minecraft:custom_name":'"Blade"',"minecraft:damage":5}},
                {Slot:1b,id:"minecraft:stone",count:64},
                {Slot:8b,id:"minecraft:ender_pearl",count:3}
            ],
            equipment:{head:{id:"minecraft:iron_helmet",count:1,components:{"minecraft:trim":{material:"minecraft:gold",pattern:"minecraft:coast"}}}},
            SelectedItemSlot:1,
            active_effects:[{id:"minecraft:speed",amplifier:1b,duration:72000,ambient:0b,show_particles:1b,show_icon:1b}],
            XpLevel:7,XpP:0.25f,XpTotal:100,XpSeed:12345,Score:4,
            EnderItems:[{Slot:3b,id:"minecraft:ender_pearl",count:5},{Slot:26b,id:"minecraft:book",count:1,components:{"minecraft:custom_name":'"Chest"'}}],
            abilities:{mayfly:1b,walkSpeed:0.12f,flySpeed:0.07f,mayBuild:1b,flying:0b,instabuild:0b,invulnerable:0b},
            respawn:{dimension:"minecraft:overworld",pos:[I;10,70,1],yaw:90.0f,pitch:0.0f,forced:1b},
            LastDeathLocation:{dimension:"minecraft:overworld",pos:[I;1,70,2]},
            entered_nether_pos:[1.5d,64.0d,2.5d],
            seenCredits:1b,
            warden_spawn_tracker:{ticks_since_last_warning:30,warning_level:2,cooldown_ticks:15},
            recipeBook:{recipes:["minecraft:stick","minecraft:oak_planks"],toBeDisplayed:["minecraft:stick"]},
            ShoulderEntityLeft:{id:"minecraft:parrot",Variant:2,Health:6.0f},
            playerGameType:0,previousPlayerGameType:1,
            foodLevel:12,foodSaturationLevel:2.0f,
            Dimension:"minecraft:overworld"
        }"#,
    )
    .expect("populated player");
    seed.put("Pos", pos.clone());
    seed.put_int(
        "DataVersion",
        mc_world::region::entity_storage::CURRENT_DATA_VERSION,
    );
    seed
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn playerdata_matches_vanilla() {
    // two servers put a fresh player at most `2 * respawn_radius` (gamerule default 10) apart; the walk adds its length
    let fresh = 2.0 * 10.0;
    let mut report = Vec::new();
    let mut standing = None;
    for (name, walk, is_populated, slack) in [
        (
            "new",
            false,
            false,
            Slack {
                pos: fresh,
                food: 1.0,
            },
        ),
        (
            "walked",
            true,
            false,
            Slack {
                pos: fresh + 8.0,
                food: f64::INFINITY,
            },
        ),
        (
            "populated",
            false,
            true,
            Slack {
                pos: 0.0,
                food: 1.0,
            },
        ),
    ] {
        let seed = is_populated
            .then(|| populated(standing.as_ref().expect("the fresh player's position")));
        let (mut v, mut o) = run_both(walk, seed).await;
        sort_unordered(&mut v);
        sort_unordered(&mut o);
        if standing.is_none() {
            standing = v.get_list("Pos").cloned();
        }
        let mut d = Vec::new();
        let rules = Rules {
            tolerance: &|p| tolerance(p, slack),
            ignore: &|_| false,
            unordered: &|_| false,
        };
        diff_compound("", &v, &o, &rules, &mut d);
        println!("=== {name}: {} differences", d.len());
        for l in &d {
            println!("{l}");
        }
        println!("--- vanilla {name}:\n{v:?}\n--- ours {name}:\n{o:?}");
        report.extend(d.into_iter().map(|l| format!("{name}{l}")));
    }
    assert!(
        report.is_empty(),
        "player data differences:\n{}",
        report.join("\n")
    );
}
