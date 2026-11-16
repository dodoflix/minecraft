//! Golden tests of `PlayerList.respawn` across the levels of a server: `tests/oracle/scenarios/respawn_*.txt`, fixtures from
//! `scripts/block-oracle.sh NAME` (the real vanilla server with all of its levels).
use crate::common::block_scenario::check;

/// A respawn anchor: charge consumed, the depletion sound, running out, uncharged, obstructed, in the overworld, destroyed, forced.
#[test]
fn respawn_anchor_cases() {
    check("respawn_anchor");
}

/// A bed: intact, destroyed, enclosed, in the nether, forced in the nether.
#[test]
fn respawn_bed_cases() {
    check("respawn_bed");
}

/// The world spawn with the respawn radius and the world border.
#[test]
fn respawn_radius_and_border() {
    check("respawn_border");
}

/// A hardcore world: the respawn request makes the player a spectator.
#[test]
fn respawn_on_a_hardcore_world() {
    check("respawn_hardcore");
}

/// A forced respawn point (`/spawnpoint`).
#[test]
fn respawn_forced_point() {
    check("respawn_forced");
}

/// A death with a menu open and a stack on the cursor.
#[test]
fn respawn_with_an_open_menu() {
    check("respawn_menu");
}

/// A death in a boat and on a horse.
#[test]
fn respawn_while_riding() {
    check("respawn_riding");
}

/// keep_inventory, immediate_respawn, effects, experience and score across a respawn.
#[test]
fn respawn_game_rules_and_state() {
    check("respawn_rules");
}

/// A death at the world spawn: the respawn packets, a living player's request ignored, the last death location.
#[test]
fn death_at_the_world_spawn() {
    check("respawn_world_spawn");
}
