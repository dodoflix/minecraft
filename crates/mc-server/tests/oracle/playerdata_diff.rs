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
    /// The height of a fresh player: the column `PlayerSpawnFinder` picks has the height of its terrain, and the seeded
    /// spawn lies beside a cliff of 50 blocks.
    y: f64,
    /// Exhaustion and saturation: a walk of wall-clock length spends food, so the walked case ignores them.
    food: f64,
}

/// How far two numbers may be apart at `path` and still count as equal: the values the clock of the run drives (the
/// food level regrows on peaceful, effect durations, the warden tracker) and `slack`.
fn tolerance(path: &str, slack: Slack) -> f64 {
    match path {
        ".Pos[1]" => slack.y,
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
    if walk {
        // the spawn lies on a slope: `PlayerSpawnFinder` starts at a random point of the radius (a thread-local
        // random) and the walk then ends 50 blocks higher or lower on either server, so the walk starts at the spawn
        console.cmd("gamerule respawn_radius 0");
    }
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
                y: f64::INFINITY,
                food: 1.0,
            },
        ),
        (
            "walked",
            true,
            false,
            Slack {
                // the walk starts at the spawn itself (`respawn_radius` 0) and adds its length
                pos: 8.0,
                y: 8.0,
                food: f64::INFINITY,
            },
        ),
        (
            "populated",
            false,
            true,
            Slack {
                pos: 0.0,
                y: 0.0,
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

/// The `Serialization errors` reports of a server log, each as `LEVEL [logger] Serialization errors:` and the lines under it.
fn serialization_reports(log: &str) -> Vec<String> {
    crate::world_start::log_records(log)
        .into_iter()
        .filter(|record| record.contains("] Serialization errors:"))
        .collect()
}

/// A player file whose `Pos` is not a list and whose `Inventory` is not a list: the join logs the decode problems of the
/// position (read twice, before the player exists and by `Entity.load`) and of the inventory, with the paths of the
/// reports.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn player_load_problems_match_vanilla() {
    let mut seed = snbt::parse_compound(r#"{Pos:"x",Inventory:5,Rotation:[1.0f]}"#).expect("seed");
    seed.put_int(
        "DataVersion",
        mc_world::region::entity_storage::CURRENT_DATA_VERSION,
    );
    // a flat world without structures: the report names the id of the player and the position it was put at, which a
    // generated world makes differ (entities created while it generates, the height of the terrain at the spawn)
    let props = ["level-type=minecraft:flat", "generate-structures=false"];
    let mut reports = Vec::new();
    for vanilla_side in [true, false] {
        let seed = seed.clone();
        let builder = if vanilla_side {
            vanilla(12345, "survival", &props)
        } else {
            ours(12345, "survival", &props)
        };
        let server = builder
            .mojang(crate::common::mojang())
            .prepare(move |dir| write_seed(dir, &seed))
            .start()
            .expect("server");
        server.cmd("gamerule respawn_radius 0");
        server
            .wait_log("respawn_radius", 30)
            .expect("the gamerule is set");
        let bot = join("127.0.0.1", server.port, "Bot").await;
        let log = server.log();
        bot.disconnect().await;
        server.stop();
        reports.push(serialization_reports(&log));
    }
    assert!(!reports[0].is_empty(), "the real server logged nothing");
    assert_eq!(reports[0], reports[1], "vanilla / ours");
}

/// A joining player whose stats file is cut, whose advancement file names an advancement that does not exist and whose
/// recipe book holds a recipe that does not exist: the three files are read in the order of the `ServerPlayer`
/// constructor (statistics, advancements) and then `load` (recipe book), and each log record names the recipe as a
/// `ResourceKey`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn player_side_file_problems_match_vanilla() {
    let mut seed = snbt::parse_compound(
        r#"{recipeBook:{recipes:["minecraft:crafting_table","minecraft:no_such_recipe"],toBeDisplayed:["minecraft:no_such_recipe"]}}"#,
    )
    .expect("seed");
    seed.put_int(
        "DataVersion",
        mc_world::region::entity_storage::CURRENT_DATA_VERSION,
    );
    let props = ["level-type=minecraft:flat", "generate-structures=false"];
    let uuid = mc_core::uuid_util::create_offline_player_uuid("Bot");
    let mut logs = Vec::new();
    for vanilla_side in [true, false] {
        let seed = seed.clone();
        let builder = if vanilla_side {
            vanilla(12345, "survival", &props)
        } else {
            ours(12345, "survival", &props)
        };
        let server = builder
            .mojang(crate::common::mojang())
            .prepare(move |dir| {
                write_seed(dir, &seed);
                let players = dir.join("world/players");
                for (folder, text) in [
                    ("stats", r#"{"stats":{"minecraft:custom":{"minecraft:jump""#),
                    (
                        "advancements",
                        r#"{"minecraft:story/no_such_advancement":{"criteria":{"a":"2020-01-01 00:00:00 +0000"},"done":true},"DataVersion":4903}"#,
                    ),
                ] {
                    std::fs::create_dir_all(players.join(folder)).expect("create the folder");
                    std::fs::write(players.join(folder).join(format!("{uuid}.json")), text)
                        .expect("write the file");
                }
            })
            .start()
            .expect("server");
        let bot = join("127.0.0.1", server.port, "Bot").await;
        let log = server.log();
        bot.disconnect().await;
        server.stop();
        logs.push(crate::world_start::log_records(&log));
    }
    let text = logs[0].join("\n");
    for expected in [
        "Couldn't parse statistics file",
        "Ignored advancement 'minecraft:story/no_such_advancement'",
        "Tried to load unrecognized recipe: ResourceKey[minecraft:recipe / minecraft:no_such_recipe]",
    ] {
        assert!(
            text.contains(expected),
            "the real server did not log {expected}:\n{text}"
        );
    }
    assert_eq!(logs[0], logs[1], "vanilla / ours");
}

const FLAT_PROPS: [&str; 2] = ["level-type=minecraft:flat", "generate-structures=false"];

/// A flat world the real server saved, so both servers load the same chunks, with the entity file of chunk (0, 0)
/// replaced by `entities` and its region chunk edited by `region`.
struct FlatWorld {
    _staging: tempfile::TempDir,
    world: std::path::PathBuf,
}

impl FlatWorld {
    fn new(region: Option<&dyn Fn(CompoundTag) -> CompoundTag>, entities: &[CompoundTag]) -> Self {
        use mc_core::ident::Identifier;
        use mc_core::pos::chunk_pos::ChunkPos;
        use mc_world::region::{RegionFileStorage, RegionStorageInfo};

        let base = vanilla(12345, "survival", &FLAT_PROPS)
            .start()
            .expect("base");
        assert!(base.stop());
        let staging = tempfile::tempdir().expect("staging");
        let world = staging.path().join("world");
        crate::world_start::copy_dir(&base.dir().join("world"), &world);
        let dimension = world.join("dimensions/minecraft/overworld");
        let edit = |folder: &str, edit: &dyn Fn(Option<CompoundTag>) -> CompoundTag| {
            let info =
                RegionStorageInfo::new("t", Identifier::parse("overworld").unwrap(), "chunk");
            let mut storage = RegionFileStorage::new(info, dimension.join(folder), false);
            let pos = ChunkPos::new(0, 0);
            let chunk = edit(storage.read(&pos).expect("read"));
            storage.write(&pos, Some(&chunk)).expect("write");
            storage.close().expect("close");
        };
        if let Some(region) = region {
            edit("region", &|chunk| {
                region(chunk.expect("the chunk of the spawn"))
            });
        }
        let version = mc_world::region::entity_storage::CURRENT_DATA_VERSION;
        edit("entities", &|_| {
            let mut chunk = CompoundTag::new();
            chunk.put_int("DataVersion", version);
            chunk.put_int_array("Position", vec![0, 0]);
            let mut list = ListTag::new();
            for e in entities {
                list.push(e.clone());
            }
            chunk.put("Entities", list);
            chunk
        });
        Self {
            _staging: staging,
            world,
        }
    }

    /// The log records of a start of both servers (vanilla, ours) on the world with `seed` as the player file and a
    /// join of the bot, sorted: chunks load on worker threads, so the order of the records of the chunk, the entities
    /// and the player is not fixed.
    async fn records(&self, seed: &CompoundTag) -> Vec<Vec<String>> {
        let mut logs = Vec::new();
        for vanilla_side in [true, false] {
            let seed = seed.clone();
            let world = self.world.clone();
            let builder = if vanilla_side {
                vanilla(12345, "survival", &FLAT_PROPS)
            } else {
                ours(12345, "survival", &FLAT_PROPS)
            };
            let server = builder
                .mojang(crate::common::mojang())
                .prepare(move |dir| {
                    crate::world_start::copy_dir(&world, &dir.join("world"));
                    write_seed(dir, &seed);
                    // a name the cache does not know is resolved through the Mojang API: a lookup no test can rely on
                    std::fs::write(
                        dir.join("usercache.json"),
                        format!(
                            r#"[{{"name":"Bot","uuid":"{}","expiresOn":"2099-01-01 00:00:00 +0000"}}]"#,
                            mc_core::uuid_util::create_offline_player_uuid("Bot")
                        ),
                    )
                    .expect("write the profile cache");
                })
                .start()
                .expect("server");
            // a fresh player is put at a random point of the respawn radius, and the report names the position
            server.cmd("gamerule respawn_radius 0");
            server
                .wait_log("respawn_radius", 30)
                .expect("the gamerule is set");
            let bot = join("127.0.0.1", server.port, "Bot").await;
            // the tracker pairs the entities with the player once his chunks are sent
            tokio::time::sleep(Duration::from_secs(2)).await;
            bot.disconnect().await;
            let log = server.log();
            server.stop();
            let mut records = crate::world_start::log_records(&log);
            // the lag of a busy machine, with the ticks it ran behind in the text
            records.retain(|r| !r.contains("Can't keep up"));
            records.sort();
            logs.push(records);
        }
        logs
    }
}

/// Fails with the records only one side logged.
fn assert_same_records(logs: &[Vec<String>]) {
    let only = |a: &[String], b: &[String]| -> Vec<String> {
        a.iter().filter(|r| !b.contains(r)).cloned().collect()
    };
    assert!(
        logs[0] == logs[1],
        "only vanilla logged:\n{}\nonly ours logged:\n{}",
        only(&logs[0], &logs[1]).join("\n"),
        only(&logs[1], &logs[0]).join("\n")
    );
}

/// Turns the first block of the lowest section into a chest and gives the chunk the block entity `chest`.
fn put_chest(chunk: &mut CompoundTag, chest: &CompoundTag) {
    let mut states =
        snbt::parse_compound(r#"{palette:["minecraft:air","minecraft:chest"]}"#).expect("palette");
    // 4 bits per block: the chest is the first block of the lowest section
    let mut data = vec![0_i64; 256];
    data[0] = 1;
    states.put_long_array("data", data);
    let section = chunk
        .get_list_mut("sections")
        .and_then(|l| l.iter_mut().next())
        .and_then(|s| s.as_compound_mut())
        .expect("lowest section");
    section.put("block_states", states);
    let mut list = ListTag::new();
    list.push(chest.clone());
    chunk.put("block_entities", list);
}

/// Item list entries that do not decode, in every place that loads them: the inventory and ender chest of a player, a
/// container block entity and a chested horse of a chunk, and the `Item` of item entities. The entries are not
/// compounds, have a `Slot` of the wrong type, a stack that fails to decode, an unknown item or a bad count. The item
/// entities keep a partial stack: one without any item is discarded by the load and the real server still tracks it (a
/// `Fetching packet for removed entity` warning that mc-server does not log), so none is part of this test.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn item_list_decode_problems_match_vanilla() {
    let version = mc_world::region::entity_storage::CURRENT_DATA_VERSION;
    let entries = r#"[
        5,
        "text",
        [1,2],
        {Slot:"x",id:"minecraft:stone",count:1},
        {Slot:2b,id:"minecraft:wither_spawn_egg",count:1,components:{"minecraft:entity_data":{id:"Pig"}}},
        {Slot:300,id:"minecraft:stone",count:1},
        {Slot:3b,id:"minecraft:no_such_item",count:1},
        {Slot:4b,id:"minecraft:stone",count:"x"},
        {Slot:5b,id:"minecraft:stone",count:0},
        {Slot:6b},
        {id:"minecraft:dirt",count:2},
        {Slot:7b,id:"minecraft:stone",count:1,components:{"minecraft:damage":"x"}}
    ]"#;
    let bad_item = r#"{id:"minecraft:wither_spawn_egg",count:1,components:{"minecraft:entity_data":{id:"Pig"}}}"#;
    let chest = snbt::parse_compound(&format!(
        r#"{{id:"minecraft:chest",x:0,y:-64,z:0,Items:{entries}}}"#
    ))
    .expect("chest");
    let entity = |id: &str, extra: &str, uuid: i32| {
        snbt::parse_compound(&format!(
            r#"{{id:"{id}",Pos:[8.5d,-60.0d,8.5d],Motion:[0.0d,0.0d,0.0d],Rotation:[0.0f,0.0f],UUID:[I;{uuid},2,3,4],{extra}}}"#
        ))
        .expect("entity")
    };
    let entities = [
        entity("minecraft:item", &format!("Item:{bad_item}"), 1),
        entity(
            "minecraft:item",
            r#"Item:{id:"minecraft:stone",count:"x"}"#,
            4,
        ),
        entity(
            "minecraft:donkey",
            &format!("ChestedHorse:1b,Items:{entries}"),
            5,
        ),
    ];
    let mut seed = snbt::parse_compound(&format!("{{Inventory:{entries},EnderItems:{entries}}}"))
        .expect("seed");
    seed.put_int("DataVersion", version);

    let world = FlatWorld::new(
        Some(&|chunk| {
            let mut chunk = chunk;
            put_chest(&mut chunk, &chest);
            chunk
        }),
        &entities,
    );
    let logs = world.records(&seed).await;
    let text = logs[0].join("\n");
    for expected in [
        "from field 'Items' at index",
        "from field 'Inventory' at index",
        "from field 'EnderItems' at index",
    ] {
        assert!(
            text.contains(expected),
            "the real server did not log {expected}:\n{text}"
        );
    }
    assert_same_records(&logs);
}

/// The `Owner` of tameable animals and horses: a legacy owner name, a short int array, an int and a name that is too
/// long to be one. Every failed decode of the field is reported once per entity, then the old name conversion runs.
/// The other uuid fields of entities (`Thrower`, `LoveCause`, `angry_at`, `last_hurt_by_*`, the owner of projectiles, a
/// vex, tnt, fangs and a cloud, the target of a shulker bullet, `ConversionPlayer`, `UUID`) are read with the same codec
/// and report the same way.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn owner_decode_problems_match_vanilla() {
    let entity = |id: &str, fields: &str, uuid: i32| {
        snbt::parse_compound(&format!(
            r#"{{id:"{id}",Pos:[8.5d,-60.0d,8.5d],Motion:[0.0d,0.0d,0.0d],Rotation:[0.0f,0.0f],UUID:[I;{uuid},2,3,4],{fields}}}"#
        ))
        .expect("entity")
    };
    let stone = r#"Item:{id:"minecraft:stone",count:1}"#;
    let entities = [
        entity("minecraft:wolf", r#"Owner:"Bot""#, 1),
        entity("minecraft:cat", "Owner:[I;1,2]", 2),
        entity("minecraft:parrot", "Owner:5", 3),
        entity("minecraft:horse", r#"Owner:"Bot""#, 4),
        entity("minecraft:donkey", "Owner:[I;1,2,3,4,5]", 5),
        entity("minecraft:mule", r#"Owner:"a_name_longer_than_sixteen""#, 6),
        entity("minecraft:wolf", "Owner:[I;1,2,3,4]", 7),
        entity(
            "minecraft:item",
            &format!(r#"{stone},Owner:"x",Thrower:[I;1,2,3]"#),
            8,
        ),
        entity(
            "minecraft:item",
            &format!("{stone},Owner:[I;1,2,3,4,5],Thrower:7"),
            9,
        ),
        entity("minecraft:cow", "LoveCause:[I;1]", 10),
        entity("minecraft:cow", r#"LoveCause:"x",angry_at:5"#, 11),
        entity("minecraft:wolf", r#"angry_at:[I;1,2,3,4,5,6]"#, 12),
        entity(
            "minecraft:zombie",
            "last_hurt_by_mob:[I;1],last_hurt_by_player:5",
            13,
        ),
        entity("minecraft:arrow", "Owner:[I;1,2]", 14),
        entity("minecraft:snowball", r#"Owner:"x""#, 15),
        entity("minecraft:vex", "owner:[I;1,2,3]", 16),
        entity("minecraft:evoker_fangs", "Owner:5", 17),
        entity("minecraft:shulker_bullet", r#"Target:"x""#, 18),
        entity("minecraft:tnt", "owner:[I;1]", 19),
        entity("minecraft:area_effect_cloud", "Owner:[I;1,2,3,4,5]", 20),
        entity(
            "minecraft:zombie_villager",
            r#"ConversionTime:100,ConversionPlayer:"x""#,
            21,
        ),
        entity("minecraft:fox", "Trusted:[[I;1,2],[I;1,2,3,4],5]", 22),
    ];
    let mut bad_uuid = entity("minecraft:cow", "Age:0", 23);
    bad_uuid.put_string("UUID", "not-an-array");
    let entities: Vec<CompoundTag> = entities.into_iter().chain([bad_uuid]).collect();
    let mut seed = snbt::parse_compound("{Inventory:[]}").expect("seed");
    seed.put_int(
        "DataVersion",
        mc_world::region::entity_storage::CURRENT_DATA_VERSION,
    );
    let logs = FlatWorld::new(None, &entities).records(&seed).await;
    let text = logs[0].join("\n");
    assert!(
        text.contains("from field 'Owner'"),
        "the real server did not report the owner:\n{text}"
    );
    assert_same_records(&logs);
}

/// An item entity whose `Item` decodes to no stack discards itself while it loads. The level still adds and tracks it
/// (the discard happened before it had a level callback), so the player who comes near gets the warning about the
/// packet of a removed entity, with the entity's description in it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn discarded_item_entities_are_tracked_like_vanilla() {
    let entity = |item: &str, uuid: i32| {
        snbt::parse_compound(&format!(
            r#"{{id:"minecraft:item",Pos:[8.5d,-60.0d,8.5d],Motion:[0.0d,0.0d,0.0d],Rotation:[0.0f,0.0f],UUID:[I;{uuid},2,3,4],Item:{item}}}"#
        ))
        .expect("entity")
    };
    let entities = [
        entity("5", 1),
        entity(r#"{id:"minecraft:no_such_item",count:1}"#, 2),
        entity(r#"{id:"minecraft:stone",count:0}"#, 3),
    ];
    let mut seed = snbt::parse_compound("{Inventory:[]}").expect("seed");
    seed.put_int(
        "DataVersion",
        mc_world::region::entity_storage::CURRENT_DATA_VERSION,
    );
    let logs = FlatWorld::new(None, &entities).records(&seed).await;
    let text = logs[0].join("\n");
    assert!(
        text.contains("Fetching packet for removed entity ItemEntity['Air'/"),
        "the real server did not warn about the removed entity:\n{text}"
    );
    assert_same_records(&logs);
}
