//! Iron and snow golems against the vanilla oracle (ticking-level replay): fighting monsters, players and repairs, villages, the snow trail,
//! snowballs and shearing. Scenarios: `tests/oracle/scenarios/mob_{iron,snow}_golem_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// A golem holds out a poppy to a villager for 400 ticks on the one-in-8000 roll.
#[test]
fn iron_golem_offers_a_flower() {
    check("mob_iron_golem_flower");
}

/// A golem hunts the nearest zombie and skeleton but not a creeper, targets what pushes it, and cracks as it takes damage.
#[test]
fn iron_golem_fights_monsters() {
    check("mob_iron_golem_fight");
}

/// Player-made golems leave players alone, village golems answer a hit, iron ingots repair, other items do not.
#[test]
fn iron_golem_players_and_repair() {
    check("mob_iron_golem_player");
}

/// Golems walk back to a village, stroll inside it, defend it against a zombie and against a player the villagers dislike.
#[test]
fn iron_golem_village_defence() {
    check("mob_iron_golem_village");
}

/// A snow golem leaves snow where it walks, none without mob griefing, and melts in a desert.
#[test]
fn snow_golem_snow_trail_and_melting() {
    check("mob_snow_golem_trail");
}

/// A snow golem throws snowballs at monsters in range only.
#[test]
fn snow_golem_shoots_monsters() {
    check("mob_snow_golem_shoot");
}

/// Shears take a snow golem's pumpkin once.
#[test]
fn snow_golem_shearing() {
    check("mob_snow_golem_shear");
}
