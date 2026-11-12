//! Scenarios of the small item classes (compass, spyglass, goat horn, debug stick, knowledge book, books, tool wear, frost
//! walker) replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn compasses_bind_to_lodestones_and_lose_vanished_targets() {
    common::block_scenario::check("itm_compass_lodestone");
}

#[test]
fn spyglass_and_goat_horn_sounds_cooldowns_and_use() {
    common::block_scenario::check("itm_spyglass_goat_horn");
}

#[test]
fn debug_stick_selects_and_cycles_block_state_properties() {
    common::block_scenario::check("itm_debug_stick");
}

#[test]
fn knowledge_books_unlock_recipes() {
    common::block_scenario::check("itm_knowledge_book");
}

#[test]
fn books_open_and_count_as_used() {
    common::block_scenario::check("itm_book_use");
}

#[test]
fn tools_wear_by_damage_per_block_and_break() {
    common::block_scenario::check("itm_tool_durability");
}

#[test]
fn frost_walker_freezes_water_under_walking_players_and_it_melts() {
    common::block_scenario::check("itm_frost_walker");
}

#[test]
fn weapons_wear_by_damage_per_attack_and_break() {
    common::block_scenario::check("itm_weapon_durability");
}

#[test]
fn frost_walker_needs_boots_ground_and_air_above() {
    common::block_scenario::check("itm_frost_walker_conditions");
}
