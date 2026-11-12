//! Goldens of block interactions no other suite observes: container open/close sounds and lid events, the bell, the
//! conduit, the lectern, the chiseled bookshelf, open-container statistics and spectators opening containers. Scenarios `tests/oracle/scenarios/blk_*.txt`
//! replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn chests_barrels_and_shulkers_play_open_and_close_sounds_and_lid_events() {
    common::block_scenario::check("blk_chest_open_close");
}

#[test]
fn bells_ring_from_the_right_side_and_villagers_hear_them() {
    common::block_scenario::check("blk_bell_ring");
}

#[test]
fn conduits_activate_with_a_prismarine_frame_and_power_players_in_water() {
    common::block_scenario::check("blk_conduit_frame");
}

#[test]
fn lecterns_take_books_turn_pages_and_pulse_redstone() {
    common::block_scenario::check("blk_lectern_book");
}

#[test]
fn chiseled_bookshelves_store_books_per_slot_and_signal_the_last_slot() {
    common::block_scenario::check("blk_chiseled_bookshelf");
}

#[test]
fn spectators_open_only_blocks_with_a_menu_provider() {
    common::block_scenario::check("blk_spectator_open");
}

#[test]
fn opening_containers_awards_the_open_statistic_even_when_the_menu_stays_closed() {
    common::block_scenario::check("blk_container_stats");
}
