//! Golden tests of `PlayerList.respawn` across the levels of a server: `tests/oracle/scenarios/respawn_*.txt`, fixtures from
//! `scripts/block-oracle.sh NAME` (the real vanilla server with all of its levels).
use crate::common::block_scenario::check;

/// A respawn anchor: charge consumed, the depletion sound, running out, uncharged, obstructed, in the overworld, destroyed, forced.
#[test]
fn respawn_anchor_cases() {
    check("respawn_anchor");
}

/// The level a player respawns in: nothing set, a point in the overworld after dying in the nether and the end, a point in the nether
/// after dying in the overworld, and a world spawn that lies in the nether.
#[test]
fn respawn_level_selection() {
    check("respawn_dimension");
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

/// The chunks the natural spawner shuffles after a respawn into a level that held no player.
#[test]
fn respawn_into_an_empty_level() {
    check("respawn_chunks");
}

/// The death effects of a player removed by the respawn and of a player removed by its own tick before the respawn.
#[test]
fn respawn_after_the_removal_of_the_dead_player() {
    check("respawn_after_removal");
}
