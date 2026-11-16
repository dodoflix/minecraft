//! Save-file parity: the same scripted console and bot actions run against the real vanilla server and `mc-server` on
//! seed 12345, both flush a save (`save-all flush`) and stop, and the two world directories are compared file by file:
//! the file list, then `level.dat`, the chunk NBT of every `region`, `entities` and `poi` file, the dimension and
//! server-wide saved data, and the per-player files, each split by file kind (`FileKind`).
//!
//! What is normalised (see [`Policy`] and [`POLICY`]): clock values (game time, day time, weather and tick counters, the
//! times of the scene), all-zero light arrays, unordered collections (attributes, the recipe book, the game rule map) and the
//! draws of unseeded randoms. Everything else is exact, including tag types and key order. Terrain that is generated
//! while the server runs is not comparable: vanilla itself differs between two runs, so the scenes use a flat
//! overworld, compare only the chunks that are full in both runs, and skip the generated nether and end chunks except
//! the one a scene changes.
//!
//! Every difference that is not a bug fixed here is a line of [`KNOWN`] with the owner and reason; a listed line that
//! no longer matches fails the scene, so the table cannot go stale. Run with `SAVE_COMPARE_KEEP=<dir>` to keep both
//! world directories of a scene under `<dir>/<scene>/{vanilla,ours}`, and with `SAVE_COMPARE_BASELINE=1` to run vanilla
//! twice and see what a run of vanilla itself varies in.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use crate::common::world_diff::{
    FileKind, Rules, diff_compound, diff_json, list_files, read_dat, read_region, sort_unordered,
};
use crate::common::*;
use crate::entity_persistence::{ask, eventually, expect};
use mc_nbt::{CompoundTag, ListTag};
use mc_testbot::harness::vanilla;
use mc_testkit::Server;

type Scene = for<'a> fn(&'a Launch) -> Pin<Box<dyn Future<Output = ()> + 'a>>;

/// Starts the server under test in the scene's world directory (again after a stop: the directory is kept).
struct Launch {
    ours: bool,
    dir: PathBuf,
    gamemode: &'static str,
    props: &'static [&'static str],
}

impl Launch {
    fn start(&self) -> Server {
        let b = if self.ours {
            ours(12345, self.gamemode, self.props)
        } else {
            vanilla(12345, self.gamemode, self.props)
        };
        b.dir(&self.dir).start().expect("server start")
    }
}

const CHUNK_LOADED: &str = "execute if loaded 8 100 8";

/// `save-all flush`, then `stop`: the state both servers write to disk.
fn finish(server: &Server) {
    expect(server, "save-all flush", "Saved the game");
    assert!(server.stop(), "stop\n{}", server.log());
}

/// One finding of the comparison: where it is and what differs.
struct Finding {
    kind: FileKind,
    file: String,
    detail: String,
}

/// Differences that are known and not fixed here: `(scene, kind, file substring, detail prefix, owner and reason)`. A
/// finding is explained when a line of its scene matches the other three; a line that matches nothing fails its scene.
const KNOWN: &[(&str, FileKind, &str, &str, &str)] = &[];

/// A normalisation applies to the files whose path contains `.0` (`""` for all) at the NBT paths ending in `.1`.
type Rule = (&'static str, &'static str);

/// The comparison rules of one scene.
struct Policy {
    /// Path prefixes (relative to the world directory) that are not compared at all: dimensions whose terrain and
    /// mobs are generated while the server runs.
    skip: &'static [&'static str],
    /// Chunk files under the prefix of which only the given chunk is compared.
    only_chunk: &'static [(&'static str, i32, i32)],
    /// Subtrees that are not compared, in every scene.
    ignore: &'static [Rule],
    /// Subtrees that are not compared in this scene.
    scene_ignore: &'static [Rule],
    /// Compounds whose key order is not compared (identity-hashed maps).
    unordered: &'static [Rule],
    /// Numbers that may differ by at most the third value (the clocks of the two runs).
    tolerance: &'static [(&'static str, &'static str, f64)],
    /// Tolerances of this scene.
    scene_tolerance: &'static [(&'static str, &'static str, f64)],
}

const SERVER_FILES: &[&str] = &["session.lock"];

/// The two runs are driven by wall-clock waits, so the tick counters differ by the time between the scene's commands.
const TICKS: f64 = 600.0;

fn nbt_findings(
    policy: &Policy,
    kind: FileKind,
    file: &str,
    v: &CompoundTag,
    o: &CompoundTag,
    out: &mut Vec<Finding>,
) {
    let hit = |rules: &[Rule], p: &str| {
        rules
            .iter()
            .any(|(f, s)| file.contains(f) && p.ends_with(s))
    };
    let tolerance = |p: &str| {
        policy
            .tolerance
            .iter()
            .chain(policy.scene_tolerance)
            .find(|(f, s, _)| file.contains(f) && p.ends_with(s))
            .map_or(0.0, |(_, _, t)| *t)
    };
    let ignore = |p: &str| hit(policy.ignore, p) || hit(policy.scene_ignore, p);
    let unordered = |p: &str| hit(policy.unordered, p);
    let rules = Rules {
        tolerance: &tolerance,
        ignore: &ignore,
        unordered: &unordered,
    };
    let (mut v, mut o) = (v.clone(), o.clone());
    sort_unordered(&mut v);
    sort_unordered(&mut o);
    let mut d = Vec::new();
    diff_compound("", &v, &o, &rules, &mut d);
    out.extend(d.into_iter().map(|detail| Finding {
        kind,
        file: file.to_owned(),
        detail,
    }));
}

/// `chunk` with the all-zero light arrays removed and the sections that held nothing else dropped. A zero array is
/// the same light as none, but whether the engine still holds one for a section depends on the order in which the
/// chunk and its neighbours were loaded (the light section below the world is created from the layer above it when
/// the chunk loads before its neighbours); two runs of the real vanilla server differ in exactly these.
fn without_zero_light(chunk: &CompoundTag) -> CompoundTag {
    let mut chunk = chunk.clone();
    if let Some(sections) = chunk.get_list("sections") {
        let mut kept = ListTag::new();
        for section in sections.iter() {
            let Some(c) = section.as_compound() else {
                continue;
            };
            let mut c = c.clone();
            for key in ["BlockLight", "SkyLight"] {
                if c.get_byte_array(key)
                    .is_some_and(|a| a.iter().all(|b| *b == 0))
                {
                    c.remove(key);
                }
            }
            if c.contains("block_states") || c.contains("BlockLight") || c.contains("SkyLight") {
                kept.push(c);
            }
        }
        chunk.put("sections", kept);
    }
    chunk
}

/// Compares two server directories (`<dir>/world/**`).
fn compare(vdir: &Path, odir: &Path, policy: &Policy) -> Vec<Finding> {
    let (vroot, oroot) = (vdir.join("world"), odir.join("world"));
    let (fv, fo) = (list_files(&vroot), list_files(&oroot));
    assert!(
        fv.contains("level.dat") && fo.contains("level.dat"),
        "a server did not save its world: {vroot:?} {oroot:?}"
    );
    let mut out = Vec::new();
    let names: BTreeSet<&String> = fv.iter().chain(fo.iter()).collect();
    for name in names {
        if SERVER_FILES.iter().any(|s| name.ends_with(s))
            || policy.skip.iter().any(|p| name.starts_with(p))
        {
            continue;
        }
        // a chunk file other than the one that holds the compared chunk is not part of the comparison
        if let Some((_, x, z)) = policy.only_chunk.iter().find(|(p, ..)| name.starts_with(p))
            && !name.ends_with(&format!("r.{}.{}.mca", x >> 5, z >> 5))
        {
            continue;
        }
        let kind = FileKind::of(name);
        let finding = |detail: String| Finding {
            kind,
            file: name.clone(),
            detail,
        };
        let only = policy
            .only_chunk
            .iter()
            .find(|(p, ..)| name.starts_with(p))
            .map(|(_, x, z)| (*x, *z));
        let present = (fv.contains(name), fo.contains(name));
        match present {
            // a chunk file of which one chunk is compared may be missing: the other chunks are not compared
            (true, false) if only.is_none() => out.push(finding("only in vanilla".into())),
            (false, true) if only.is_none() => out.push(finding("only in ours".into())),
            _ => {
                let (pv, po) = (vroot.join(name), oroot.join(name));
                if name.ends_with(".mca") {
                    let read = |present: bool, path: &Path| {
                        if present {
                            read_region(path)
                        } else {
                            Default::default()
                        }
                    };
                    let (cv, co) = (read(present.0, &pv), read(present.1, &po));
                    let full = |c: &CompoundTag| c.get_string("Status") == Some("minecraft:full");
                    let keys: BTreeSet<&(i32, i32)> = cv.keys().chain(co.keys()).collect();
                    for k in keys {
                        if only.is_some_and(|o| o != *k) {
                            continue;
                        }
                        let (v, o) = (cv.get(k), co.get(k));
                        let (Some(v), Some(o)) = (v, o) else {
                            if kind == FileKind::Region {
                                // which chunks around the loaded area are full when the server stops depends on
                                // timing; the origin chunk is the one every scene loads
                                if *k == (0, 0) && v.or(o).is_some_and(full) {
                                    out.push(finding(format!("chunk {k:?} full in one run only")));
                                }
                            } else {
                                let who = if v.is_some() { "vanilla" } else { "ours" };
                                out.push(finding(format!("chunk {k:?} only in {who}")));
                            }
                            continue;
                        };
                        if kind == FileKind::Region && !(full(v) && full(o)) {
                            if *k == (0, 0) && full(v) != full(o) {
                                out.push(finding(format!("chunk {k:?} full in one run only")));
                            }
                            continue;
                        }
                        let mut found = Vec::new();
                        nbt_findings(
                            policy,
                            kind,
                            name,
                            &without_zero_light(v),
                            &without_zero_light(o),
                            &mut found,
                        );
                        out.extend(found.into_iter().map(|mut f| {
                            f.detail = format!("chunk {k:?}{}", f.detail);
                            f
                        }));
                    }
                } else if name.ends_with(".dat") {
                    nbt_findings(policy, kind, name, &read_dat(&pv), &read_dat(&po), &mut out);
                } else if name.ends_with(".json") {
                    let read = |p: &Path| -> serde_json::Value {
                        serde_json::from_slice(&std::fs::read(p).expect("read json"))
                            .expect("parse json")
                    };
                    let tolerance = |p: &str| {
                        policy
                            .tolerance
                            .iter()
                            .chain(policy.scene_tolerance)
                            .find(|(f, s, _)| name.contains(f) && p.ends_with(s))
                            .map_or(0.0, |(_, _, t)| *t)
                    };
                    let ignore = |p: &str| {
                        policy
                            .ignore
                            .iter()
                            .chain(policy.scene_ignore)
                            .any(|(f, s)| name.contains(f) && p.ends_with(s))
                    };
                    let rules = Rules {
                        tolerance: &tolerance,
                        ignore: &ignore,
                        unordered: &|_| false,
                    };
                    let mut d = Vec::new();
                    diff_json("", &read(&pv), &read(&po), &rules, &mut d);
                    out.extend(d.into_iter().map(finding));
                }
            }
        }
    }
    out
}

/// Runs `scene` on both servers and compares the saved worlds; fails on every finding no [`KNOWN`] line explains.
async fn run(
    name: &str,
    scene: Scene,
    launch: (&'static str, &'static [&'static str]),
    policy: &Policy,
) {
    let keep = std::env::var_os("SAVE_COMPARE_KEEP").map(PathBuf::from);
    let temp = tempfile::tempdir().expect("temp dir");
    let base = keep.map_or_else(|| temp.path().join(name), |k| k.join(name));
    let _ = std::fs::remove_dir_all(&base);
    let dirs: Vec<PathBuf> = ["vanilla", "ours"].iter().map(|n| base.join(n)).collect();
    // `SAVE_COMPARE_BASELINE=1`: the second run is vanilla too, so the differences are what a run of vanilla varies in
    let baseline = std::env::var_os("SAVE_COMPARE_BASELINE").is_some();
    for (ours, dir) in [false, !baseline].into_iter().zip(&dirs) {
        let l = Launch {
            ours,
            dir: dir.clone(),
            gamemode: launch.0,
            props: launch.1,
        };
        scene(&l).await;
    }
    let findings = compare(&dirs[0], &dirs[1], policy);
    let mut by_kind: std::collections::BTreeMap<FileKind, Vec<&Finding>> = Default::default();
    for f in &findings {
        by_kind.entry(f.kind).or_default().push(f);
    }
    let mut used = vec![false; KNOWN.len()];
    let mut unexplained = Vec::new();
    for (kind, list) in &by_kind {
        println!("=== {name} {kind:?}: {} differences", list.len());
        for f in list {
            let known = KNOWN.iter().position(|(scene, k, file, detail, _)| {
                *scene == name
                    && *k == f.kind
                    && f.file.contains(file)
                    && f.detail.starts_with(detail)
            });
            match known {
                Some(i) => {
                    used[i] = true;
                    println!("  known: {}: {}", f.file, f.detail);
                }
                None => {
                    println!("  {}: {}", f.file, f.detail);
                    unexplained.push(format!("{kind:?} {}: {}", f.file, f.detail));
                }
            }
        }
    }
    let stale: Vec<_> = KNOWN
        .iter()
        .zip(&used)
        .filter(|(k, u)| k.0 == name && !**u)
        .map(|(k, _)| format!("{:?} {} {}", k.1, k.2, k.3))
        .collect();
    assert!(
        unexplained.is_empty() && stale.is_empty(),
        "{name}: unexplained differences:\n{}\nstale known entries:\n{}",
        unexplained.join("\n"),
        stale.join("\n")
    );
}

/// A flat overworld: its terrain is the same in every run (vanilla's generated terrain differs between two runs: features of neighbouring chunks run in a varying order).
const FLAT: &[&str] = &["level-type=minecraft:flat", "generate-structures=false"];

/// The weather timers of a scene that sets no weather: draws of an unseeded random.
const UNSEEDED_WEATHER: &[Rule] = &[
    ("weather.dat", ".data.rain_time"),
    ("weather.dat", ".data.thunder_time"),
];

const POLICY: Policy = Policy {
    skip: &[],
    only_chunk: &[],
    scene_ignore: &[],
    scene_tolerance: &[],
    ignore: &[
        // wall-clock time of the save
        ("level.dat", ".Data.LastPlayed"),
        // random draws of unseeded randoms
        ("players/data", ".XpSeed"),
        ("ender_dragon_fight.dat", ".data.dragon_uuid"),
    ],
    // the game rule map is an identity-hashed `Reference2ObjectOpenHashMap`: its order differs between two vanilla runs
    unordered: &[("game_rules.dat", ".data")],
    tolerance: &[
        ("level.dat", ".Data.Time", TICKS),
        ("world_clocks.dat", ".total_ticks", TICKS),
        ("weather.dat", "_time", TICKS),
        ("world_border.dat", ".lerp_time", TICKS),
        (".mca", ".LastUpdate", TICKS),
        (".mca", ".LastGossipDecay", TICKS),
        (".mca", ".InhabitedTime", TICKS),
        // the growth of a baby animal, and the regeneration of a player that was damaged, run on the clock
        (".mca", ".Age", TICKS),
        ("players/data", ".Health", 3.0),
        ("players/data", ".foodExhaustionLevel", 4.0),
        ("players/data", ".foodTickTimer", 80.0),
        ("raids.dat", ".data.tick", TICKS),
        (
            ".dat",
            ".warden_spawn_tracker.ticks_since_last_warning",
            TICKS,
        ),
        (".dat", ".active_effects[0].duration", TICKS),
        // the player's position when it left the overworld: a fresh player is put at a random place of the spawn area
        ("players/data", ".entered_nether_pos[0]", 20.0),
        ("players/data", ".entered_nether_pos[2]", 20.0),
        ("stats", ".minecraft:play_time", TICKS),
        ("stats", ".minecraft:time_since_death", TICKS),
        ("stats", ".minecraft:time_since_rest", TICKS),
        ("stats", ".minecraft:total_world_time", TICKS),
    ],
};

fn blocks_and_settings(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        expect(&s, "forceload add 0 0", "Marked chunk");
        eventually(&s, CHUNK_LOADED, "Test passed");
        for c in [
            "setblock 8 100 8 minecraft:stone",
            "fill 0 100 0 6 103 6 minecraft:oak_planks hollow",
            "setblock 3 101 3 minecraft:chest[facing=north]{Items:[{Slot:0b,id:\"minecraft:diamond\",count:3}],CustomName:'\"Box\"'}",
            "setblock 5 101 3 minecraft:oak_sign{front_text:{messages:['\"a\"','\"b\"','\"\"','\"\"']}}",
            "setblock 1 101 1 minecraft:furnace{Items:[{Slot:0b,id:\"minecraft:coal\",count:5}]}",
            "setblock 2 101 5 minecraft:spawner{SpawnData:{entity:{id:\"minecraft:pig\"}}}",
            "gamerule keep_inventory true",
            "gamerule random_tick_speed 7",
            "weather rain 12000",
            "time set 1000",
            "setworldspawn 4 70 4",
            "worldborder center 10 10",
            "worldborder set 1000",
            "worldborder damage amount 0.5",
        ] {
            ask(&s, c);
        }
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_blocks_and_settings() {
    run(
        "blocks_and_settings",
        blocks_and_settings,
        ("survival", FLAT),
        &POLICY,
    )
    .await;
}

/// Runs the commands in order, each one waiting for its answer.
fn cmds(s: &Server, commands: &[&str]) {
    for c in commands {
        ask(s, c);
    }
}

fn entities(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        expect(&s, "forceload add 0 0", "Marked chunk");
        eventually(&s, CHUNK_LOADED, "Test passed");
        expect(&s, "setblock 8 100 8 minecraft:stone", "Changed the block");
        cmds(
            &s,
            &[
                "summon minecraft:pig 8.5 100 2.5 {UUID:[I;1,0,0,1],Tags:[\"kept\"],NoAI:1b,NoGravity:1b,PersistenceRequired:1b,CustomName:'\"Pig\"',Health:7.0f}",
                "summon minecraft:item 9.5 100 3.5 {UUID:[I;1,0,0,2],Tags:[\"kept\"],NoGravity:1b,PickupDelay:32767s,Age:-32768s,Item:{id:\"minecraft:diamond\",count:3,components:{\"minecraft:custom_name\":'\"Gem\"'}}}",
                "summon minecraft:item_frame 8 100 9 {UUID:[I;1,0,0,3],Tags:[\"kept\"],Facing:3b,ItemRotation:2b,Item:{id:\"minecraft:apple\",count:1}}",
                "summon minecraft:minecart 2.5 100 12.5 {UUID:[I;1,0,0,4],Tags:[\"kept\"],NoGravity:1b,Passengers:[{id:\"minecraft:chicken\",UUID:[I;1,0,0,5],Tags:[\"kept\"],NoAI:1b,PersistenceRequired:1b,Silent:1b}]}",
                "summon minecraft:villager 12.5 100 12.5 {UUID:[I;1,0,0,6],Tags:[\"kept\"],NoAI:1b,NoGravity:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:2},Offers:{Recipes:[{buy:{id:\"minecraft:wheat\",count:20},sell:{id:\"minecraft:emerald\",count:1},maxUses:16}]}}",
                "summon minecraft:armor_stand 4.5 100 4.5 {UUID:[I;1,0,0,7],Tags:[\"kept\"],NoGravity:1b,ShowArms:1b,Pose:{Head:[10f,0f,0f]},equipment:{mainhand:{id:\"minecraft:stick\",count:1},head:{id:\"minecraft:leather_helmet\",count:1,components:{\"minecraft:dyed_color\":255}}}}",
                "summon minecraft:wolf 6.5 100 6.5 {UUID:[I;1,0,0,8],Tags:[\"kept\"],NoAI:1b,PersistenceRequired:1b,Owner:[I;5,6,7,8],Sitting:1b,CollarColor:3b}",
                "summon minecraft:sheep 1.5 100 6.5 {UUID:[I;1,0,0,9],Tags:[\"kept\"],NoAI:1b,PersistenceRequired:1b,Color:5b,Sheared:1b,Age:-24000}",
                "summon minecraft:text_display 3.5 101 3.5 {UUID:[I;1,0,0,10],Tags:[\"kept\"],text:'\"hi\"',billboard:\"center\"}",
                "summon minecraft:marker 5.5 101 5.5 {UUID:[I;1,0,0,11],Tags:[\"kept\"],data:{a:1}}",
                "summon minecraft:interaction 7.5 101 7.5 {UUID:[I;1,0,0,12],Tags:[\"kept\"],width:1f,height:2f,response:1b}",
                "setblock 10 100 10 minecraft:composter",
                "setblock 12 100 10 minecraft:bell[attachment=floor]",
                "setblock 14 100 10 minecraft:lodestone",
                "setblock 14 100 12 minecraft:bee_nest",
            ],
        );
        eventually(
            &s,
            "execute if entity @e[tag=kept]",
            "Test passed. Count: 12",
        );
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_entities_and_poi() {
    run(
        "entities",
        entities,
        ("survival", FLAT),
        &Policy {
            // the egg timer of the chicken is a draw of an unseeded random
            scene_ignore: &[
                ("weather.dat", ".data.rain_time"),
                ("weather.dat", ".data.thunder_time"),
                (".mca", ".EggLayTime"),
            ],
            ..POLICY
        },
    )
    .await;
}

fn saved_data(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        cmds(
            &s,
            &[
                "scoreboard objectives add kills dummy",
                "scoreboard objectives add health health",
                "scoreboard objectives modify kills displayname {\"text\":\"Kills\",\"color\":\"red\"}",
                "scoreboard objectives setdisplay sidebar kills",
                "scoreboard players set Alice kills 5",
                "scoreboard players set Bob kills 7",
                "team add red",
                "team modify red color red",
                "team modify red prefix {\"text\":\"[R] \"}",
                "team modify red friendlyFire false",
                "team join red Alice",
                "bossbar add test:bar {\"text\":\"Bar\"}",
                "bossbar set test:bar max 200",
                "bossbar set test:bar value 50",
                "bossbar set test:bar color green",
                "bossbar set test:bar style notched_10",
                "bossbar set test:bar visible false",
                "data modify storage test:s a set value {x:1,y:[1,2,3],z:\"text\"}",
                "data modify storage other:t b set value 5b",
                "random value 1..100 test:seq",
                "random value 1..100 test:seq",
                "forceload add 100 100",
                "worldborder set 1000 3600",
                "gamerule keep_inventory true",
                "gamerule advance_time false",
                "time set 6000",
                "weather thunder 3000",
                "setworldspawn 12 70 -4 45",
                "difficulty hard",
                "defaultgamemode creative",
            ],
        );
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_server_saved_data() {
    run(
        "saved_data",
        saved_data,
        ("survival", FLAT),
        &Policy {
            // a border moving over an hour: one second of skew moves it by about 17 000 blocks
            scene_tolerance: &[("world_border.dat", ".size", 200_000.0)],
            ..POLICY
        },
    )
    .await;
}

fn generated_terrain(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        expect(&s, "forceload add 0 0 20 20", "Marked");
        eventually(&s, CHUNK_LOADED, "Test passed");
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_generated_terrain() {
    run(
        "generated_terrain",
        generated_terrain,
        ("survival", &["generate-structures=false"]),
        &Policy {
            // generated terrain and the animals spawned with it
            skip: &[
                "dimensions/minecraft/overworld/region",
                "dimensions/minecraft/overworld/entities",
            ],
            scene_ignore: &[
                ("weather.dat", ".data.rain_time"),
                ("weather.dat", ".data.thunder_time"),
                // the height of the spawn point is the height of generated terrain
                ("level.dat", ".Data.spawn.pos"),
            ],
            ..POLICY
        },
    )
    .await;
}

/// The int-array form of the offline uuid of "Bot", for `Owner` tags.
fn bot_uuid_ints() -> String {
    let u = mc_core::uuid_util::create_offline_player_uuid("Bot").as_u128();
    format!(
        "[I;{},{},{},{}]",
        (u >> 96) as u32 as i32,
        (u >> 64) as u32 as i32,
        (u >> 32) as u32 as i32,
        u as u32 as i32
    )
}

fn player(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        expect(&s, "forceload add 0 0", "Marked chunk");
        eventually(&s, CHUNK_LOADED, "Test passed");
        expect(&s, "setblock 8 100 8 minecraft:stone", "Changed the block");
        let bot = join("127.0.0.1", s.port, "Bot").await;
        cmds(
            &s,
            &[
                "op Bot",
                "tp Bot 8.5 101 8.5",
                "give Bot minecraft:diamond_sword[minecraft:enchantments={sharpness:3},minecraft:custom_name='\"Blade\"'] 1",
                "give Bot minecraft:ender_pearl 5",
                "effect give Bot minecraft:speed 100000 1",
                "xp add Bot 7 levels",
                "xp add Bot 3 points",
                "item replace entity Bot enderchest.3 with minecraft:book",
                "spawnpoint Bot 10 70 10 90",
                "advancement grant Bot only minecraft:story/root",
                "advancement grant Bot only minecraft:adventure/root",
                "recipe give Bot minecraft:stick",
                "recipe give Bot minecraft:oak_planks",
                "attribute Bot minecraft:max_health base set 30",
                "damage Bot 3 minecraft:generic",
            ],
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
        bot.disconnect().await;
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_player_files() {
    run(
        "player",
        player,
        ("survival", FLAT),
        &Policy {
            scene_ignore: UNSEEDED_WEATHER,
            ..POLICY
        },
    )
    .await;
}

fn nether_logout(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        let nether = "execute in minecraft:the_nether run";
        expect(&s, &format!("{nether} forceload add 0 0"), "Marked chunk");
        eventually(
            &s,
            &format!("{nether} execute if loaded 0 100 0"),
            "Test passed",
        );
        cmds(
            &s,
            &[
                &format!("{nether} fill -2 99 -2 2 99 2 minecraft:obsidian"),
                &format!("{nether} fill -2 100 -2 2 103 2 minecraft:air"),
            ],
        );
        let bot = join("127.0.0.1", s.port, "Bot").await;
        cmds(
            &s,
            &[
                "op Bot",
                &format!("{nether} tp Bot 0.5 100 0.5"),
                &format!(
                    "{nether} summon minecraft:ender_pearl 0.5 102 0.5 {{UUID:[I;2,0,0,1],Owner:{},NoGravity:1b,Motion:[0.0,0.0,0.0]}}",
                    bot_uuid_ints()
                ),
                "give Bot minecraft:ender_pearl 3",
            ],
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
        bot.disconnect().await;
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_player_logged_out_in_the_nether() {
    run(
        "nether_logout",
        nether_logout,
        ("survival", FLAT),
        &Policy {
            skip: &[
                "dimensions/minecraft/the_nether/region",
                "dimensions/minecraft/the_nether/poi",
            ],
            only_chunk: &[("dimensions/minecraft/the_nether/entities", 0, 0)],
            scene_ignore: UNSEEDED_WEATHER,
            ..POLICY
        },
    )
    .await;
}

/// Three levels visited, a stop, one saved dimension deleted, a restart that loads the player into the missing one.
fn three_levels_restart(l: &Launch) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
        let s = l.start();
        for dim in ["the_nether", "the_end"] {
            expect(
                &s,
                &format!("execute in minecraft:{dim} run forceload add 0 0"),
                "Marked chunk",
            );
        }
        let bot = join("127.0.0.1", s.port, "Bot").await;
        cmds(
            &s,
            &[
                "op Bot",
                "execute in minecraft:the_nether run tp Bot 0.5 100 0.5",
                "execute in minecraft:the_end run tp Bot 100.5 50 0.5",
            ],
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
        bot.disconnect().await;
        finish(&s);
        std::fs::remove_dir_all(l.dir.join("world/dimensions/minecraft/the_end"))
            .expect("remove the end");
        let s = l.start();
        let bot = join("127.0.0.1", s.port, "Bot").await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        bot.disconnect().await;
        finish(&s);
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn save_compare_three_levels_and_a_missing_dimension() {
    run(
        "three_levels",
        three_levels_restart,
        ("survival", FLAT),
        &Policy {
            skip: &[
                "dimensions/minecraft/the_nether/region",
                "dimensions/minecraft/the_nether/poi",
                "dimensions/minecraft/the_nether/entities",
                "dimensions/minecraft/the_end/region",
                "dimensions/minecraft/the_end/poi",
                "dimensions/minecraft/the_end/entities",
            ],
            scene_ignore: UNSEEDED_WEATHER,
            ..POLICY
        },
    )
    .await;
}
