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
    "minecraft:sniffer",
    "minecraft:witch",
    // the `aggr` field is 0 where vanilla shows 1
    "minecraft:panda",
    // `yHeadRot` (head=0.49275765 in vanilla, 4.9479904 here) and the saved data differ
    "minecraft:zombie_villager",
];

/// `mob_finalize_summon` and `mob_spawn_egg_finalize`: inert placeholders and the class-specific differences of single types.
const FINALIZE_SUMMON_DIVERGENT: &[&str] = &[
    // inert placeholders: the dump line stops after the position fields
    "minecraft:allay",
    "minecraft:bee",
    "minecraft:camel",
    "minecraft:evoker",
    "minecraft:ravager",
    "minecraft:sniffer",
    "minecraft:wandering_trader",
    "minecraft:witch",
    // the entity is loaded from its tag (`/summon`, an egg's `entity_data`) before `finalizeSpawn`; the load ends in
    // `setTarget(null)`, whose speed attribute lookup creates the saved attribute instance (`movement_speed` without modifiers)
    "minecraft:enderman",
    // the saved `anger` data holds `suspects:[]` where vanilla saves `{}`
    "minecraft:warden",
    // the horse data (`Bred`, `EatingHaystack`, `Tame`, `Temper`) is not saved
    "minecraft:zombie_horse",
    // `yHeadRot` draws after another number of entity randoms; `ConversionTime`, `VillagerData`, `Xp` are not saved
    "minecraft:zombie_villager",
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
    // The villager brain applies its schedule at construction (`core;rest` at day time 0), here at the first AI step: the
    // active activities are the only field skipped, so a villager's type, profession and saved data stay compared.
    let villager_opts = Options {
        ignore_fields: vec!["acts"],
        ..Options {
            ignore_kinds: vec!["C"],
            ..Default::default()
        }
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
        let opts = if ty == "minecraft:villager" {
            &villager_opts
        } else {
            &opts
        };
        let same =
            exp.len() == act.len() && exp.iter().zip(act).all(|(e, a)| lines_match(e, a, opts));
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
fn mob_villager_brain() {
    block_scenario::check("mob_villager_brain");
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
