//! Container click, creative slot and close packets through the player's real packet handlers. Scenarios
//! `tests/oracle/scenarios/mnu_*.txt` replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our level.

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
