//! Menu packets through the player's real packet handlers: container clicks, creative slots, closing, renaming and menu buttons on the
//! chest, enchanting table, anvil and grindstone. Scenarios `tests/oracle/scenarios/mnu_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our level.

use crate::common;

#[test]
fn clicks_on_a_chest_menu_change_slots_cursor_and_drops_like_vanilla() {
    common::block_scenario::check("mnu_click_chest");
}

#[test]
fn click_packets_are_checked_against_the_menu_state_before_they_apply() {
    common::block_scenario::check("mnu_click_sync");
}

#[test]
fn creative_slot_packets_set_copy_clear_and_drop_stacks() {
    common::block_scenario::check("mnu_creative_slot");
}

#[test]
fn dropping_in_creative_mode_is_throttled_per_tick_window() {
    common::block_scenario::check("mnu_creative_throttle");
}

#[test]
fn closing_a_menu_drops_the_cursor_stack() {
    common::block_scenario::check("mnu_close_cursor");
}

#[test]
fn the_join_sends_recipe_properties_book_and_inventory_menu_before_any_menu_packet() {
    common::block_scenario::check("mnu_join_burst");
}

#[test]
fn rename_beacon_and_trade_packets_reach_the_open_menu() {
    common::block_scenario::check("mnu_menu_packets");
}

#[test]
fn the_enchanting_table_costs_levels_and_lapis_and_advances_the_seed() {
    common::block_scenario::check("mnu_enchant_table");
}

#[test]
fn the_anvil_renames_repairs_and_combines_through_the_packet_handlers() {
    common::block_scenario::check("mnu_anvil_rename");
}

#[test]
fn the_grindstone_removes_enchantments_and_pays_out_experience() {
    common::block_scenario::check("mnu_grindstone");
}

#[test]
fn picking_an_entity_selects_or_adds_its_pick_result_and_prints_avatar_profiles() {
    common::block_scenario::check("mnu_pick_entity");
}
