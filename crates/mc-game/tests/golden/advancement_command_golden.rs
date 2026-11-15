//! `/advancement`, the `advancements` selector option and the `SelectedItem` tag of players against the real vanilla level: feedback
//! lines, completed criteria and advancement packets. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with
//! `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn advancement_command_modes_match_vanilla() {
    common::block_scenario::check("cmd_advancement_modes");
}

#[test]
fn advancements_selector_option_matches_vanilla() {
    common::block_scenario::check("cmd_selector_advancements");
}

#[test]
fn selected_item_nbt_matches_vanilla() {
    common::block_scenario::check("cmd_selected_item_nbt");
}
