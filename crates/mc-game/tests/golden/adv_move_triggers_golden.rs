//! Advancement criteria fired by movement and position against the vanilla oracle (ticking-level replay, data pack `tests/oracle/datapacks/adv_move`):
//! falls, levitation, effect changes, location, riding in lava, blocks entered, sleeping and avoided vibrations. Scenarios:
//! `tests/oracle/scenarios/adv_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// The fall distance reset awards `fall_from_height` in the tick of the reset: a landing, water, a cobweb, a ladder, slow falling, a boat, an upward
/// movement packet, a creative player and a drift sideways; a landing that kills awards nothing.
#[test]
fn falls_award_the_player_when_the_fall_distance_resets() {
    check("adv_fall_height");
}

/// Levitation distance and duration conditions: an effect that runs on, expires, is cleared, is upgraded in place, meets a roof or starts late.
#[test]
fn levitation_awards_the_player_every_tick() {
    check("adv_levitation");
}

/// Effects added, upgraded, hidden, cleared, expired and wiped; the effect conditions and the source entity of the change.
#[test]
fn effect_changes_award_the_player() {
    check("adv_effects_changed");
}

/// The once-per-second location check against biome, dimension, block, fluid, sky, smoke, light, position and stepped-on block conditions.
#[test]
fn location_awards_the_player_every_second() {
    check("adv_location_triggers");
}

/// The structure and biome conditions of the location check inside and outside a generated swamp hut.
#[test]
fn location_matches_generated_structures() {
    check("adv_location_structures");
}

/// A vehicle in lava: the entry position, the distance every tick, leaving the lava and entering it again.
#[test]
fn riding_in_lava_awards_the_player() {
    check("adv_ride_lava");
}

/// Blocks a player stands in (with their states), sleeping in a bed by night, by day and with a zombie near.
#[test]
fn entered_blocks_and_beds_award_the_player() {
    check("adv_enter_block_sleep");
}

/// A sculk sensor ignoring a sneaking player (landing, walking, jumping, out of range) awards it; one that is set off by a walking player does not.
#[test]
fn ignored_vibrations_award_the_sneaking_player() {
    check("adv_avoid_vibration");
}
