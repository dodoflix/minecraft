//! Replays of the recorded mob scenarios (`oracle/scenarios/mob_*.txt`, `aquatic_*.txt`) against their vanilla fixtures.
use crate::common::block_scenario;
use crate::common::entity_oracle::{Fixture, Options, lines_match};
use crate::common::entity_scenario::{run_scenario, run_scenario_test};
use std::collections::BTreeMap;

/// The mob types whose `finalizeSpawn` block (rich `E` line + saved NBT) differs from vanilla today. The test fails when a type
/// outside the list diverges and when a listed type no longer does (delete its entry), so the list can only shrink.
const FINALIZE_EQUIPMENT_DIVERGENT: &[&str] = &[
    // inert placeholders: the dump line stops after the position fields
    "minecraft:camel",
    "minecraft:sniffer",
    // the `aggr` field is 0 where vanilla shows 1
    "minecraft:panda",
];

/// `mob_finalize_summon` and `mob_spawn_egg_finalize`: inert placeholders and the class-specific differences of single types.
const FINALIZE_SUMMON_DIVERGENT: &[&str] = &[
    // inert placeholders: the dump line stops after the position fields
    "minecraft:allay",
    "minecraft:camel",
    "minecraft:sniffer",
    "minecraft:wandering_trader",
    // the saved `anger` data holds `suspects:[]` where vanilla saves `{}`
    "minecraft:warden",
];

/// Every `mobspawn` block of `mob_finalize_equipment` (208 mobs of about 52 types, each with its own difficulty and spawn reason).
#[test]
fn mob_finalize_equipment() {
    let actual = run_scenario("mob_finalize_equipment");
    check_finalize_blocks(
        "mob_finalize_equipment",
        &actual,
        FINALIZE_EQUIPMENT_DIVERGENT,
    );
}

/// Compares the `E` and `N` lines of every named mob of a finalize scenario and checks that the types whose blocks differ are
/// exactly `divergent`. The test fails when a type outside the list diverges and when a listed type no longer does.
fn check_finalize_blocks(name: &str, actual: &[String], divergent: &[&str]) {
    let fixture = Fixture::load(name);
    let opts = Options {
        ignore_kinds: vec!["C"],
        ..Default::default()
    };
    // mob name -> (type, expected lines, actual lines)
    let mut blocks: BTreeMap<String, (String, Vec<&String>, Vec<&String>)> = BTreeMap::new();
    for (lines, expected) in [(&fixture.lines[..], true), (actual, false)] {
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
    let mut got: BTreeMap<&str, &str> = BTreeMap::new();
    for (mob, (ty, exp, act)) in &blocks {
        let same =
            exp.len() == act.len() && exp.iter().zip(act).all(|(e, a)| lines_match(e, a, &opts));
        if !same {
            got.entry(ty).or_insert(mob);
        }
    }
    let mut want = divergent.to_vec();
    want.sort_unstable();
    assert_eq!(
        got.keys().copied().collect::<Vec<_>>(),
        want,
        "types whose finalizeSpawn block differs (first block of each: {got:?})"
    );
}

/// `/summon` of every mob type, every other spawn reason and shared spawn group data (block replay).
#[test]
fn mob_finalize_summon() {
    let actual = block_scenario::run_scenario("mob_finalize_summon");
    check_finalize_blocks("mob_finalize_summon", &actual, FINALIZE_SUMMON_DIVERGENT);
}

/// Spawn eggs, mob buckets, the villager variant component and spawners holding a bare id (block replay).
#[test]
fn mob_spawn_egg_finalize() {
    let actual = block_scenario::run_scenario("mob_spawn_egg_finalize");
    check_finalize_blocks("mob_spawn_egg_finalize", &actual, FINALIZE_SUMMON_DIVERGENT);
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
#[ignore = "deviation: the allay is an inert placeholder, `path_query` finds no flying path where vanilla finds one (line 50: `PATHQ Allay ... found=true`)"]
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
fn mob_villager_brain() {
    block_scenario::check("mob_villager_brain");
}

#[test]
fn aquatic_fish_swim() {
    run_scenario_test("aquatic_fish_swim");
}

#[test]
fn aquatic_squid_swim() {
    run_scenario_test("aquatic_squid_swim");
}

#[test]
fn mob_raid_start() {
    block_scenario::check("mob_raid_start");
}

#[test]
fn mob_raid_waves() {
    run_scenario_test("mob_raid_waves");
}

#[test]
fn mob_chunkgen_plains() {
    block_scenario::check("mob_chunkgen_plains");
}

#[test]
fn mob_chunkgen_village() {
    block_scenario::check("mob_chunkgen_village");
}

#[test]
fn mob_place_village() {
    block_scenario::check("mob_place_village");
}

#[test]
fn mob_chunkgen_snowy() {
    block_scenario::check("mob_chunkgen_snowy");
}

#[test]
#[ignore = "deviation: two mooshrooms at 70,72,13 and 70,72,15 (chunk 4,0, outside the preload) exist here after boot where vanilla's chunk is not yet full"]
fn mob_chunkgen_mushroom() {
    block_scenario::check("mob_chunkgen_mushroom");
}

#[test]
fn mob_spawnpass_plains_day() {
    block_scenario::check("mob_spawnpass_plains_day");
}

#[test]
fn mob_spawnpass_plains_night() {
    block_scenario::check("mob_spawnpass_plains_night");
}

#[test]
fn mob_spawnpass_biomes() {
    block_scenario::check("mob_spawnpass_biomes");
}

#[test]
fn mob_spawnpass_cap() {
    block_scenario::check("mob_spawnpass_cap");
}

#[test]
fn mob_spawnpass_hard_night() {
    block_scenario::check("mob_spawnpass_hard_night");
}

#[test]
fn mob_spawnpass_peaceful() {
    block_scenario::check("mob_spawnpass_peaceful");
}

#[test]
fn mob_spawnpass_respawn_point() {
    block_scenario::check("mob_spawnpass_respawn_point");
}

#[test]
#[ignore = "deviation: the fortress list is found (238 lines match: wither skeletons, blazes, piglins) but the block light of the replayed nether differs from vanilla near the lava inside the fortress (block light 3 here, 10 in vanilla at -68,58,156), so a dark-enough spawn test passes here where vanilla rejects it (SPAWNAT seed=491 at -66,58,154)"]
fn mob_spawnpass_fortress() {
    block_scenario::check("mob_spawnpass_fortress");
}

#[test]
fn mob_spawnpass_structures_monument() {
    block_scenario::check_until("mob_spawnpass_structures_monument", "spawn attempts");
}

#[test]
#[ignore = "deviation: an attempt inside the monument succeeds here where vanilla's is blocked (SPAWNAT seed=344 at 232,54,184: the guardians of the preceding attempts differ in position), guardian movement"]
fn mob_spawnpass_structures_monument_attempts() {
    block_scenario::check("mob_spawnpass_structures_monument");
}

#[test]
fn mob_spawnpass_structures_outpost() {
    block_scenario::check_until("mob_spawnpass_structures_outpost", "spawn attempts");
}

#[test]
#[ignore = "deviation: pillagers are inert placeholders: after the attempts vanilla's pillagers look around and walk (line 249: head=-50.837383 vs 0.0)"]
fn mob_spawnpass_structures_outpost_attempts() {
    block_scenario::check("mob_spawnpass_structures_outpost");
}

#[test]
fn mob_spawnpass_structures_swamp_hut() {
    block_scenario::check_until("mob_spawnpass_structures_swamp_hut", "spawn attempts");
}

#[test]
fn mob_spawnpass_structures_swamp_hut_attempts() {
    block_scenario::check("mob_spawnpass_structures_swamp_hut");
}

#[test]
fn mob_spawnpass_water() {
    block_scenario::check("mob_spawnpass_water");
}

/// One `PhantomSpawner` tick with the level random seeded (block replay: the spawner runs on the ticking level).
#[test]
fn mob_custom_spawner_phantom() {
    block_scenario::check("mob_custom_spawner_phantom");
}

/// Brain memories and activities set from the scenario (`memory`, `activity`).
#[test]
fn mob_brain_isolation() {
    run_scenario_test("mob_brain_isolation");
}
