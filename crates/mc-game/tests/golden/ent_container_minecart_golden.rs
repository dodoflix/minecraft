//! Chest and hopper minecarts against vanilla: slowdown by fill, hopper suction, opening and menus, loot tables, destruction and the
//! comparator signal of a detector rail (`tests/oracle/scenarios/mc_old_{chest,hopper}.txt`, `ent_container_minecart_*.txt`,
//! `ent_hopper_minecart_suck.txt`, `blk_rails_detector_analog.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn chest_carts_slow_down_by_their_fill() {
    common::block_scenario::check("mc_old_chest");
}

#[test]
fn hopper_cart_sucks_a_chest_and_item_entities_while_rolling() {
    common::block_scenario::check("mc_old_hopper");
}

#[test]
fn opening_container_carts() {
    common::block_scenario::check("ent_container_minecart_open");
}

#[test]
fn breaking_container_carts() {
    common::block_scenario::check("ent_container_minecart_destroy");
}

#[test]
fn piglins_get_angry_when_a_player_breaks_a_container_cart() {
    common::block_scenario::check("ent_container_minecart_anger");
}

#[test]
fn loot_tables_of_container_carts() {
    common::block_scenario::check("ent_container_minecart_loot");
}

#[test]
fn container_cart_menus() {
    common::block_scenario::check("ent_container_minecart_menu");
}

#[test]
fn hopper_cart_suction_and_activator_rails() {
    common::block_scenario::check("ent_hopper_minecart_suck");
}

#[test]
fn detector_rail_comparator_output() {
    common::block_scenario::check("blk_rails_detector_analog");
}
