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

/// A mob hurt by a player that then changes level still holds it as the last attacker and hunts it.
#[test]
fn last_hurt_by_mob_is_a_player_of_another_level() {
    check("lvl_last_hurt_by_remote_player");
}

/// Two animals in love through a player of another level still count the birth for that player.
#[test]
fn love_cause_is_a_player_of_another_level() {
    check("lvl_love_cause_remote_player");
}

/// The kill of a lingering potion's cloud is credited to its owner, who is in another level.
#[test]
fn area_effect_cloud_owner_is_in_another_level() {
    check("lvl_cloud_owner_remote");
}

/// Thorns worn by the victim of an arrow of a player in another level hurts that player, with her own random.
#[test]
fn thorns_hurt_the_arrow_owner_in_another_level() {
    check("lvl_thorns_remote_owner");
}

/// Evoker fangs of an evoker in another level hurt with that evoker as the cause, spare its team, and the thorns they meet hurt the evoker.
#[test]
fn fangs_owner_is_a_mob_in_another_level() {
    check("lvl_fangs_owner_remote_mob");
}

/// Evoker fangs of a player in another level: the player is the cause and the kill credit; its team is spared.
#[test]
fn fangs_owner_is_a_player_in_another_level() {
    check("lvl_fangs_owner_remote_player");
}

/// A shulker bullet keeps homing in on a player or a mob in another level, and falls for a spectator or an unknown target.
#[test]
fn shulker_bullet_target_is_in_another_level() {
    check("lvl_shulker_bullet_target_remote");
}

/// An item thrown by a player who is in another level fires the thrown-item pickup triggers for that player.
#[test]
fn thrown_item_pickup_triggers_for_a_thrower_in_another_level() {
    check("lvl_thrown_pickup_remote_thrower");
}

/// `execute on owner` reaches the owner of a horse in another level.
#[test]
fn horse_owner_is_in_another_level() {
    check("lvl_horse_owner_remote");
}

/// What the hurt of thorns does with the level of the arrow (a shield's block sound, infested silverfish) happens in that level, not the owner's.
#[test]
fn thorns_hurt_effects_happen_in_the_level_of_the_effect() {
    check("lvl_thorns_remote_owner_effects");
}

/// A small fireball whose owner is a mob in another level counts as a mob's: it lights no fire with mob griefing off.
#[test]
fn small_fireball_owner_is_a_mob_in_another_level() {
    check("lvl_small_fireball_owner_remote_mob");
}

/// A phantom that was swooping at a player who went to another level loses its target and never bites.
#[test]
fn phantom_loses_a_target_that_left_its_level() {
    check("lvl_phantom_swoop_remote_player");
}

/// The blindness spell of an illusioner reaches its target in another level.
#[test]
fn illusioner_blinds_a_target_in_another_level() {
    check("lvl_illusioner_blindness_remote_player");
}

/// A dolphin swimming with a player who went to another level refreshes the grace of that player with the random of the player's level.
#[test]
fn dolphin_grace_reaches_a_player_in_another_level() {
    check("lvl_dolphin_grace_remote_player");
}
