//! Behaviour that reaches into another level of the server, against vanilla (`tests/oracle/scenarios/lvl_*.txt`; fixtures
//! regenerated with `scripts/entity-oracle.sh`).

use crate::common::block_scenario::check;

/// A player killed by an arrow of a player in another level gets the entity_killed_player trigger in the same blow.
#[test]
fn kill_credit_of_a_player_in_another_level() {
    check("lvl_kill_player_other_level");
}

/// The killer in another level is judged with its armor, held items, effects, experience, team and tags.
#[test]
fn kill_credit_reads_the_equipment_of_a_player_in_another_level() {
    check("lvl_kill_player_remote_gear");
}

/// A loot table reads the attacker, the attacking player and the damage source's entity of another level the same way.
#[test]
fn loot_table_reads_the_equipment_of_a_killer_in_another_level() {
    check("lvl_loot_remote_killer");
}

/// A player's attack packet finds no entity for the id of a player in another level, even one that a mob of this level hunts.
#[test]
fn packets_do_not_reach_a_player_of_another_level() {
    check("lvl_attack_packet_other_level");
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
