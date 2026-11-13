//! The stonecutter menu: the recipe lists, buttons, result slot and clicks of the real vanilla menu (in-process script
//! `oracle/menu_scripts/mnu_stonecutter.txt`, `scripts/menu-oracle.sh mnu_stonecutter`) and the block opening it for real players through the
//! packet handlers (scenario `oracle/scenarios/mnu_open_stonecutter.txt`, `scripts/block-oracle.sh mnu_open_stonecutter`).

use crate::common;
use crate::common::menu_oracle::replay;

#[test]
fn the_stonecutter_menu_lists_selects_crafts_and_closes_like_vanilla() {
    let ops = replay("menu_oracle", "mnu_stonecutter");
    assert!(ops > 800, "fixture has {ops} ops");
}

#[test]
fn the_stonecutter_block_opens_its_menu_and_crafts_through_the_packet_handlers() {
    common::block_scenario::check("mnu_open_stonecutter");
}
