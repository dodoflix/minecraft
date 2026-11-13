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

#[test]
fn the_loom_opens_takes_its_result_and_returns_its_inputs() {
    common::block_scenario::check("mnu_open_loom");
}

#[test]
fn button_packets_reach_the_open_menu_only() {
    common::block_scenario::check("mnu_button_verb");
}

#[test]
fn the_cartography_table_opens_for_survival_spectator_and_creative_players() {
    common::block_scenario::check("mnu_open_cartography");
}

#[test]
fn the_crafter_toggles_slots_through_the_slot_state_packet() {
    common::block_scenario::check("mnu_crafter_slots");
}

#[test]
fn the_furnaces_open_with_their_titles_locks_and_statistics() {
    common::block_scenario::check("mnu_open_furnaces");
}

#[test]
fn taking_a_furnace_result_pays_experience_and_unlocks_the_recipes_used() {
    common::block_scenario::check("mnu_furnace_take");
}

#[test]
fn the_recipe_book_places_recipes_into_the_furnaces() {
    common::block_scenario::check("mnu_place_recipe_furnace");
}

#[test]
fn survival_players_open_dispensers_hoppers_shulker_boxes_barrels_and_crafters_and_locks_refuse_them()
 {
    common::block_scenario::check("mnu_container_open_locks");
}

#[test]
fn a_double_chest_opens_the_nine_by_six_menu_and_breaking_a_half_closes_it() {
    common::block_scenario::check("mnu_double_chest");
}

#[test]
fn the_enchanting_offers_ignore_bookshelves_removed_after_they_were_computed() {
    common::block_scenario::check("mnu_enchant_stale_power");
}

#[test]
fn the_ender_chest_menu_persists_per_player_across_chests() {
    common::block_scenario::check("mnu_ender_chest_menu");
}

#[test]
fn a_dead_players_crafting_grids_drop_when_the_menu_is_removed() {
    common::block_scenario::check("mnu_grid_death_drop");
}

#[test]
fn hoppers_feeding_a_furnace_update_its_open_menu() {
    common::block_scenario::check("mnu_hopper_furnace_open");
}

#[test]
fn menus_close_beyond_the_block_interaction_range_plus_buffer() {
    common::block_scenario::check("mnu_menu_range");
}

#[test]
fn pick_block_moves_adds_or_ignores_items_by_game_mode_and_reach() {
    common::block_scenario::check("mnu_pick_block_variants");
}

#[test]
fn picking_items_up_unlocks_their_recipes() {
    common::block_scenario::check("mnu_recipe_unlock_pickup");
}

#[test]
fn sneaking_with_an_empty_hand_opens_menu_blocks_and_an_item_skips_them() {
    common::block_scenario::check("mnu_sneak_use");
}

#[test]
fn limited_crafting_at_a_crafting_table_needs_known_recipes_except_special_ones() {
    common::block_scenario::check("mnu_table_limited_unlock");
}

#[test]
fn taking_results_awards_statistics_recipes_and_advancements() {
    common::block_scenario::check("mnu_take_awards");
}

#[test]
fn villagers_that_cannot_trade_now_open_no_trade_screen() {
    common::block_scenario::check("mnu_trade_two_players");
}
