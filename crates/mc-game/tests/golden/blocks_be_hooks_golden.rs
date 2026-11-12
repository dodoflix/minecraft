//! Block entity and use hooks that differ when run in the wrong place: scenarios `tests/oracle/scenarios/blk_*.txt` replayed on
//! the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/`).

use crate::common;

#[test]
fn redstone_ore_lights_under_a_walking_player_but_not_a_sneaking_player_or_crouching_cat() {
    common::block_scenario::check("blk_redstone_ore_step");
}

#[test]
fn shulker_box_lid_does_not_push_marker_armor_stands() {
    common::block_scenario::check("blk_shulker_marker_push");
}

#[test]
fn creaking_heart_comparator_reads_the_live_block_entity() {
    common::block_scenario::check("blk_creaking_heart_signal");
}

#[test]
fn end_gateway_set_changed_updates_comparators() {
    common::block_scenario::check("blk_end_gateway_changed");
}

#[test]
fn jukebox_data_merge_with_another_record_stops_the_song() {
    common::block_scenario::check("blk_jukebox_data_merge");
}

#[test]
fn loom_stonecutter_and_tables_consume_the_use() {
    common::block_scenario::check("blk_workstation_use");
}

#[test]
fn lectern_resolves_a_placed_written_book_without_a_player() {
    common::block_scenario::check("blk_lectern_book_resolve");
}

#[test]
fn jigsaw_generate_advances_the_level_random() {
    common::block_scenario::check("blk_jigsaw_generate_random");
}
