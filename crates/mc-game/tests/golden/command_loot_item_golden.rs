//! `/loot` and `/item` against the real vanilla level: every loot source and target, the response messages, the slot commands and the
//! item modifiers. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn item_bundles_match_vanilla() {
    common::block_scenario::check("cmd_item_bundles");
}

#[test]
fn item_modify_functions_match_vanilla() {
    common::block_scenario::check("cmd_item_modify_functions");
}

#[test]
fn item_slots_match_vanilla() {
    common::block_scenario::check("cmd_item_slots");
}

#[test]
fn loot_give_matches_vanilla() {
    common::block_scenario::check("cmd_loot_give");
}

#[test]
fn loot_inline_tables_match_vanilla() {
    common::block_scenario::check("cmd_loot_inline");
}

#[test]
fn loot_insert_matches_vanilla() {
    common::block_scenario::check("cmd_loot_insert");
}

#[test]
fn loot_replace_matches_vanilla() {
    common::block_scenario::check("cmd_loot_replace");
}

#[test]
fn loot_spawn_matches_vanilla() {
    common::block_scenario::check("cmd_loot_spawn");
}

#[test]
fn luck_changes_loot_rolls_like_vanilla() {
    common::block_scenario::check("cmd_loot_luck");
}
