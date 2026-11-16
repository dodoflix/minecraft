//! Behaviour that reaches into another level of the server, against vanilla (`tests/oracle/scenarios/lvl_*.txt`; fixtures
//! regenerated with `scripts/entity-oracle.sh`).

use crate::common::block_scenario::check;

/// A player killed by an arrow of a player in another level gets the entity_killed_player trigger in the same blow.
#[test]
fn kill_credit_of_a_player_in_another_level() {
    check("lvl_kill_player_other_level");
}

/// An experience orb keeps following a player that left its level, died or respawned, while the player's last position stays within 8 blocks.
#[test]
fn experience_orb_follows_a_player_of_another_level() {
    check("lvl_orb_follows_player_elsewhere");
}

#[test]
fn experience_orb_follows_a_dead_player_object() {
    check("lvl_orb_follows_dead_player");
}
