//! Replays of the recorded mob scenarios (`oracle/scenarios/mob_*.txt`, `aquatic_*.txt`) against their vanilla fixtures.
use crate::common::block_scenario;
use crate::common::entity_oracle::{Fixture, Options, lines_match};
use crate::common::entity_scenario::{run_scenario, run_scenario_test};
use std::collections::BTreeMap;

/// The mob types whose `finalizeSpawn` block (rich `E` line + saved NBT) differs from vanilla today. The test fails when a type
/// outside the list diverges and when a listed type no longer does (delete its entry), so the list can only shrink.
const FINALIZE_EQUIPMENT_DIVERGENT: &[&str] = &[
    // inert placeholders: the dump line stops after the position fields
    "minecraft:bee",
    "minecraft:camel",
    "minecraft:evoker",
    "minecraft:goat",
    "minecraft:pillager",
    "minecraft:sniffer",
    "minecraft:vindicator",
    "minecraft:witch",
    // the saved `anger_end_time` is -1 where vanilla saves 0
    "minecraft:enderman",
    "minecraft:iron_golem",
    "minecraft:polar_bear",
    "minecraft:zombified_piglin",
    // the `aggr` field is 0 where vanilla shows 1
    "minecraft:panda",
    // `yHeadRot` (head=0.49275765 in vanilla, 4.9479904 here) and the saved data differ
    "minecraft:zombie_villager",
];

/// Every `mobspawn` block of `mob_finalize_equipment` (208 mobs of about 52 types, each with its own difficulty and spawn reason).
#[test]
fn mob_finalize_equipment() {
    let fixture = Fixture::load("mob_finalize_equipment");
    let actual = run_scenario("mob_finalize_equipment");
    let opts = Options {
        ignore_kinds: vec!["C"],
        ..Default::default()
    };
    // name -> (type, expected lines, actual lines)
    let mut blocks: BTreeMap<String, (String, Vec<&String>, Vec<&String>)> = BTreeMap::new();
    for (lines, expected) in [(&fixture.lines, true), (&actual, false)] {
        for l in lines {
            let t: Vec<&str> = l.split(' ').collect();
            if !matches!(t[0], "E" | "N") {
                continue;
            }
            let b = blocks.entry(t[1].to_string()).or_default();
            if t[0] == "E" {
                b.0 = t[2].to_string();
            }
            if expected {
                b.1.push(l);
            } else {
                b.2.push(l);
            }
        }
    }
    let mut divergent: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, (ty, exp, act)) in &blocks {
        let same =
            exp.len() == act.len() && exp.iter().zip(act).all(|(e, a)| lines_match(e, a, &opts));
        if !same {
            divergent.entry(ty).or_insert(name);
        }
    }
    let got: Vec<&str> = divergent.keys().copied().collect();
    let mut want = FINALIZE_EQUIPMENT_DIVERGENT.to_vec();
    want.sort_unstable();
    assert_eq!(
        got, want,
        "types whose finalizeSpawn block differs (first block of each: {divergent:?})"
    );
}

#[test]
fn mob_local_difficulty() {
    block_scenario::check("mob_local_difficulty");
}

#[test]
fn mob_nav_follow() {
    run_scenario_test("mob_nav_follow");
}

#[test]
fn mob_path_climb() {
    run_scenario_test("mob_path_climb");
}

#[test]
#[ignore = "deviation: the bee is an inert placeholder, `path_query` finds no flying path where vanilla finds one (line 0: `PATHQ Bee ... found=true`)"]
fn mob_path_fly() {
    run_scenario_test("mob_path_fly");
}

#[test]
fn mob_path_water() {
    run_scenario_test("mob_path_water");
}

#[test]
fn mob_player_attack() {
    run_scenario_test("mob_player_attack");
}

#[test]
fn mob_skeleton_shoot() {
    run_scenario_test("mob_skeleton_shoot");
}

#[test]
#[ignore = "deviation: from tick 7 vanilla keeps the `visible_mobs` memory as an empty list (`mem=visible_mobs:():inf`), ours has no memory"]
fn mob_villager_brain() {
    run_scenario_test("mob_villager_brain");
}

#[test]
#[ignore = "deviation: goal names of nested classes drop the outer class (`AbstractFish$FishSwimGoal` in vanilla); from tick 21 a salmon path starts at another node (`nodes=14,-57,-5` vs `15,-57,-4`)"]
fn aquatic_fish_swim() {
    run_scenario_test("aquatic_fish_swim");
}

#[test]
#[ignore = "deviation: goal names of nested classes drop the outer class (`Squid$SquidRandomMovementGoal` in vanilla); everything else matches"]
fn aquatic_squid_swim() {
    run_scenario_test("aquatic_squid_swim");
}

#[test]
#[ignore = "deviation: the raid's first wave (tick 170) panics: the raiders are inert placeholder entities (patrolling_monster.rs:35)"]
fn mob_raid_start() {
    block_scenario::check("mob_raid_start");
}

#[test]
fn mob_raid_waves() {
    run_scenario_test("mob_raid_waves");
}

#[test]
#[ignore = "deviation: the pass of seed 22 (chunk 1,-2) spawns pigs in vanilla (line 11: `E pig#1`), none here"]
fn mob_spawnpass_plains_day() {
    block_scenario::check("mob_spawnpass_plains_day");
}

#[test]
#[ignore = "deviation: the pass of seed 22 (chunk 1,-2) spawns nothing in vanilla (line 7: `SPAWNSTATE`), here a zombie, a spider and a skeleton"]
fn mob_spawnpass_plains_night() {
    block_scenario::check("mob_spawnpass_plains_night");
}

#[test]
#[ignore = "deviation: the pass of seed 126 (chunk -1,-3) spawns nothing in vanilla (line 5: `SPAWNSTATE`), here pigs and horses"]
fn mob_spawnpass_biomes() {
    block_scenario::check("mob_spawnpass_biomes");
}

/// One `PhantomSpawner` tick with the level random seeded (block replay: the spawner runs on the ticking level).
#[test]
#[ignore = "deviation: the first PhantomSpawner tick spawns a phantom in vanilla (line 1: `E phantom#1 ... pos=8.5,-40.0,2.5`) and leaves `Level.random` at 155513899567442, here nothing and 205572635213177"]
fn mob_custom_spawner_phantom() {
    block_scenario::check("mob_custom_spawner_phantom");
}

/// Brain memories and activities set from the scenario (`memory`, `activity`).
#[test]
fn mob_brain_isolation() {
    run_scenario_test("mob_brain_isolation");
}
