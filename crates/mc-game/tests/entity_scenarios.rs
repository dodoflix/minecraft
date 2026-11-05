//! Replays the committed entity-oracle scenarios through the generic scenario harness (`common/entity_scenario.rs`) and compares them
//! with the vanilla fixtures. Known gaps of the Rust server are listed as `ignore_fields` per scenario (remove entries as they are fixed).
//!
//! Adding a mob golden: scenario in `oracle/scenarios/NAME.txt`, `scripts/entity-oracle.sh NAME.txt -o fixtures/entity_oracle/NAME.txt`,
//! then `run_scenario_test("NAME")` (see the harness header).
mod common;
use common::entity_oracle::Options;
use common::entity_scenario::*;

/// `Entity.baseTick` leaves `remainingFireTicks` at `-getFireImmuneTicks()` (-20) in vanilla; the Rust base tick keeps 0.
fn gaps() -> Options {
    Options { ignore_fields: vec!["fire"], ..Default::default() }
}

#[test]
fn fall() {
    run_scenario_test_with("fall", &gaps());
}

#[test]
fn jump() {
    run_scenario_test_with("jump", &gaps());
}

/// Rust gap (EntitiesCore2): from tick 68 (player at the water surface, eye y=-50.2 vs surface -50) vanilla keeps draining the air supply
/// while our `LivingEntity` starts refilling it (eye-in-fluid evaluation), which changes the whole trajectory afterwards.
#[test]
#[ignore = "known Rust gap: eyeInFluid/air supply at the water surface diverges from tick 68"]
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
    let lines = run_scenario_text("inline", "level flat\nnote hello\nspawn_item a minecraft:diamond 0.5 -50 0.5 0 0 0\ndump entity:a\ntick 2\n");
    assert_eq!(lines[0], "# hello");
    assert!(lines[1].starts_with("E a minecraft:item pos=0.5,-50.0,0.5 "), "{}", lines[1]);
    assert_eq!(lines.iter().filter(|l| l.starts_with("T ")).count(), 0, "no watch: no T headers");
}

/// Example mob golden (the ten minute recipe in the harness header): needs the zombie of the mob slice, until then it only documents
/// the three steps (scenario + fixture exist already, comparison fails on the missing mob fields).
#[test]
#[ignore = "example: enable when ZOMBIE + the mob dump registrations exist"]
fn example_mob_golden_zombie_chase() {
    run_scenario_test("mob_zombie_chase");
}
