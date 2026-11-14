//! Goldens of the aquatic mobs: dolphin (swimming, feeding, jumping, the structure search of a fed dolphin), pufferfish puffing and
//! stinging, schooling fish groups, salmon and tropical fish predicates, turtle egg laying and hatching, and every fish bucket.
//! Scenarios: `tests/oracle/scenarios/{aquatic_dolphin_*,mob_dolphin_*,mob_pufferfish_puff,mob_fish_*,mob_turtle_egg,mob_bucket_all}.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario;
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn dolphin_swim_matches_vanilla() {
    run_scenario_test("aquatic_dolphin_swim");
}

#[test]
fn dolphin_feed_matches_vanilla() {
    run_scenario_test("aquatic_dolphin_feed");
}

#[test]
fn dolphin_jump_matches_vanilla() {
    block_scenario::check("mob_dolphin_jump");
}

/// A fed dolphin in a seeded world with structures swims towards the nearest shipwreck.
#[test]
fn dolphin_treasure_matches_vanilla() {
    block_scenario::check("mob_dolphin_treasure");
}

#[test]
fn dolphin_treasure_without_structures_matches_vanilla() {
    run_scenario_test("mob_dolphin_treasure_nostructure");
}

#[test]
fn pufferfish_puff_matches_vanilla() {
    block_scenario::check("mob_pufferfish_puff");
}

#[test]
fn glow_squid_dark_matches_vanilla() {
    run_scenario_test("mob_glow_squid_dark");
}

#[test]
fn fish_school_matches_vanilla() {
    run_scenario_test("mob_fish_school");
}

/// Entity predicates over the salmon size and tropical fish pattern and colour components.
#[test]
fn fish_predicate_matches_vanilla() {
    block_scenario::check("mob_fish_predicate");
}

#[test]
fn turtle_egg_matches_vanilla() {
    block_scenario::check("mob_turtle_egg");
}

/// A water bucket on every fish picks it up, the filled bucket releases it again.
#[test]
fn bucket_all_matches_vanilla() {
    block_scenario::check("mob_bucket_all");
}
