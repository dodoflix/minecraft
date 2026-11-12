//! Container clicks that fail, armor slots and `allowed_entities`, and the open-menu lifecycle through the player's real packet
//! handlers. Scenarios `tests/oracle/scenarios/mnu_*.txt` replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our level.

use crate::common;

#[test]
fn a_click_that_throws_leaves_the_remote_updates_suppressed() {
    common::block_scenario::check("mnu_click_error_path");
}

#[test]
fn armor_slots_check_the_allowed_entities_of_the_equippable_component() {
    common::block_scenario::check("mnu_equippable_allowed_entities");
}

#[test]
fn opening_closing_and_losing_a_menu_matches_vanilla() {
    common::block_scenario::check("mnu_lifecycle");
}
