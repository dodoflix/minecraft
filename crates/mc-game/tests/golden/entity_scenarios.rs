//! Replays the committed entity-oracle scenarios through the generic scenario harness (`common/entity_scenario.rs`) and compares them
//! with the vanilla fixtures. Known gaps of the Rust server are listed as `ignore_fields` per scenario (remove entries as they are fixed).
//!
//! Adding a mob golden: scenario in `oracle/scenarios/NAME.txt`, `scripts/entity-oracle.sh NAME.txt -o fixtures/entity_oracle/NAME.txt`,
//! then `run_scenario_test("NAME")` (see the harness header).
use crate::common::entity_oracle::Options;
use crate::common::entity_scenario::*;

/// `Entity.baseTick` leaves `remainingFireTicks` at `-getFireImmuneTicks()` (-20) in vanilla; the Rust base tick keeps 0.
fn gaps() -> Options {
    Options {
        ignore_fields: vec!["fire"],
        ..Default::default()
    }
}

#[test]
fn fall() {
    run_scenario_test_with("fall", &gaps());
}

#[test]
fn jump() {
    run_scenario_test_with("jump", &gaps());
}

/// From tick 68 (player at the water surface, eye y=-50.2 vs surface -50) vanilla keeps draining the air supply
/// while our `LivingEntity` starts refilling it (eye-in-fluid evaluation), which changes the whole trajectory afterwards.
#[test]
#[ignore = "deviation: air supply refills at the water surface where vanilla keeps draining (#6)"]
fn swim() {
    run_scenario_test_with("swim", &gaps());
}

#[test]
fn item_fall() {
    run_scenario_test_with("item_fall", &gaps());
}

#[test]
fn item_merge() {
    run_scenario_test_with("item_merge", &gaps());
}

#[test]
fn item_collide() {
    run_scenario_test_with("item_collide", &gaps());
}

#[test]
fn item_motion() {
    run_scenario_test_with("item_motion", &gaps());
}

#[test]
#[should_panic(expected = "unsupported scenario command `frobnicate`")]
fn unknown_commands_fail_loudly() {
    run_scenario_text("inline", "level void\nfrobnicate 1 2 3\n");
}

#[test]
fn plain_entities_and_notes_replay() {
    let lines = run_scenario_text(
        "inline",
        "level flat\nnote hello\nspawn_item a minecraft:diamond 0.5 -50 0.5 0 0 0\ndump entity:a\ntick 2\n",
    );
    assert_eq!(lines[0], "# hello");
    assert!(
        lines[1].starts_with("E a minecraft:item pos=0.5,-50.0,0.5 "),
        "{}",
        lines[1]
    );
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("T ")).count(),
        0,
        "no watch: no T headers"
    );
}

/// Example mob golden (the recipe in the harness header): a zombie chasing a player.
#[test]
#[ignore = "deviation: the scenario replay lacks the mob_drops rule and command feedback lines (#7)"]
fn example_mob_golden_zombie_chase() {
    run_scenario_test("mob_zombie_chase");
}

/// Trade generation through the real `Villager` (profession x level x seed x type): the 130 villager lines of `mob_trades`.
#[test]
fn mob_trades_villagers() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let scenario =
        std::fs::read_to_string(dir.join("oracle/scenarios/mob_trades.txt")).expect("scenario");
    let villagers: String = scenario
        .lines()
        .take_while(|l| !l.starts_with("mobspawn W"))
        .map(|l| format!("{l}\n"))
        .collect();
    let expected = std::fs::read_to_string(dir.join("fixtures/entity_oracle/mob_trades.txt"))
        .expect("fixture");
    let expected: Vec<&str> = expected
        .lines()
        .take_while(|l| !l.contains(" W1 "))
        .collect();
    let actual = run_scenario_text("mob_trades_villagers", &villagers);
    let actual: Vec<&str> = actual
        .iter()
        .map(String::as_str)
        .filter(|l| l.starts_with("OFFER"))
        .collect();
    let expected: Vec<&str> = expected
        .into_iter()
        .filter(|l| l.starts_with("OFFER"))
        .collect();
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(a, e, "line {i}");
    }
    assert_eq!(actual.len(), expected.len());
}

/// Smoke test of the mob command arms (`mobspawn love age breed target nav path_query tame leash interact` + `goals:`/`path:`/`attrs:`/`nbt:`
/// dumps): they must run without panicking on the real mobs of the server. Exactness is checked by the mob goldens (`mob_*` fixtures).
#[test]
fn mob_command_arms_run() {
    let lines = run_scenario_text(
        "inline",
        "level flat\ndifficulty normal\nspawn_player Steve survival 0.5 -60 0.5 0 0\n\
         mobspawn P1 minecraft:pig 4.5 -60 4.5 0 0 nofinalize\nmobspawn P2 minecraft:pig 5.5 -60 4.5 0 0 nofinalize\nmobspawn W minecraft:wolf 2.5 -60 0.5 0 0 nofinalize\n\
         tick 2\nlove P1 Steve\nlove P2 Steve\nbreed P1 P2\ndump entities\nage P1 -100\ntarget W Steve\ntarget W -\n\
         path_query P1 8.5 -60 4.5\nnav P2 8.5 -60 6.5 1.0 1\ntick 3\ndump goals:P1 path:P2 attrs:W nbt:W\ntame W Steve\nleash P1 Steve\nleash P1 -\ngive Steve mainhand minecraft:wheat 4\ninteract Steve P2\n",
    );
    assert!(
        lines.iter().any(|l| l.starts_with("E pig#1 ")),
        "the baby is named pig#1: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("PATHQ P1 "))
            && lines.iter().any(|l| l.starts_with("NAV P2 "))
    );
    assert!(
        lines.iter().any(|l| l.starts_with("G P1 goal "))
            && lines.iter().any(|l| l.starts_with("ATTR W "))
            && lines.iter().any(|l| l.starts_with("N W {"))
    );
}

/// Path goldens of the mobs.
#[test]
#[ignore = "deviation: a cow finds no walkable path across the river (#8)"]
fn mob_path_ground() {
    run_scenario_test("mob_path_ground");
}

/// Piglin barter / admire / hostility toward a player without gold armor.
#[test]
#[ignore = "deviation: the scenario replay lacks the mob_drops rule and command feedback lines (#7)"]
fn mob_piglin_barter() {
    run_scenario_test("mob_piglin_barter");
}

/// Hoglin attack and throwTarget knockback against a player.
#[test]
#[ignore = "deviation: the scenario replay lacks the mob_drops rule and command feedback lines (#7)"]
fn mob_hoglin_vs_player() {
    run_scenario_test("mob_hoglin_vs_player");
}
