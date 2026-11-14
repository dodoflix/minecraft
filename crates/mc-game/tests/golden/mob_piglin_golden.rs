//! The piglin family against the vanilla oracle (ticking-level replay): admiring and bartering, hunting hoglins, brutes, conversion in
//! the overworld, crossbows, anger at chests and gold blocks, retaliation and baby piglins.
//! Scenarios: `tests/oracle/scenarios/mob_piglin_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario;

/// A piglin admires a gold ingot for 119 ticks, throws the barter loot toward the player and takes a golden sword into its hand; a
/// player in gold armor is left alone.
#[test]
fn piglin_admires_and_barters() {
    block_scenario::check("mob_piglin_admire_throw");
}

/// A piglin with a sword hunts and kills an adult hoglin, dances at the spot and does not hunt again for a while.
#[test]
fn piglin_hunts_hoglins() {
    block_scenario::check("mob_piglin_hunt_hoglin");
}

/// A piglin brute attacks a player in gold armor, ignores gold, and returns to its home when the player is gone.
#[test]
fn piglin_brute_attacks_and_goes_home() {
    block_scenario::check("mob_piglin_brute_home");
}

/// Piglins that are not immune turn into zombified piglins 300 ticks after they started counting in the overworld.
#[test]
fn piglins_zombify_in_the_overworld() {
    block_scenario::check("mob_piglin_zombify");
}

/// A piglin with a crossbow charges and shoots at a player in view.
#[test]
fn piglin_shoots_a_crossbow() {
    block_scenario::check("mob_piglin_crossbow");
}

/// Opening a chest angers the adult piglins that see the player, and the anger spreads.
#[test]
fn piglins_get_angry_at_chests() {
    block_scenario::check("mob_piglin_anger_chest");
}

/// Breaking a gold block angers the adult piglins that see the player.
#[test]
fn piglins_get_angry_at_gold() {
    block_scenario::check("mob_piglin_anger_gold");
}

/// A hit piglin and the adults around it hunt the attacker, and forget it after a while.
#[test]
fn piglins_retaliate() {
    block_scenario::check("mob_piglin_retaliate");
}

/// A baby piglin avoids zombified piglins, wanders next to a baby hoglin and admires gold.
#[test]
fn baby_piglin_behaviour() {
    block_scenario::check("mob_piglin_baby");
}
