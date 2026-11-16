//! The entity and inventory commands (`give`, `clear`, `enchant`, `experience`, `attribute`, `damage`, `effect`, `kill`, `summon`,
//! `teleport`, `rotate`, `ride`, `spectate`, `swing`, `tag`, `gamemode`, `defaultgamemode`, `title`) against the real vanilla level: the
//! feedback, failure and result lines, the player, inventory and entity state they leave behind and the packets the players receive.
//! Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn attribute_and_damage_match_vanilla() {
    common::block_scenario::check("cmd_attribute_damage");
}

#[test]
fn effect_kill_and_summon_match_vanilla() {
    common::block_scenario::check("cmd_effect_kill_summon");
}

#[test]
fn enchant_and_experience_match_vanilla() {
    common::block_scenario::check("cmd_enchant_xp");
}

#[test]
fn gamemode_and_title_match_vanilla() {
    common::block_scenario::check("cmd_gamemode_title");
}

#[test]
fn give_and_clear_match_vanilla() {
    common::block_scenario::check("cmd_give_clear");
}

#[test]
fn ride_spectate_swing_and_tag_match_vanilla() {
    common::block_scenario::check("cmd_ride_spectate_tag");
}

#[test]
fn teleport_and_rotate_match_vanilla() {
    common::block_scenario::check("cmd_teleport_rotate");
}

/// `/tp` goes on with the entity it started with: a non-player that changed dimension is a new entity that keeps its momentum.
#[test]
fn teleport_after_effects_follow_the_original_entity() {
    common::block_scenario::check("cmd_tp_after_teleport");
}
