//! Container block hooks against vanilla recordings: trapped chest neighbour updates, shulker box lid animation and
//! loot unpacking, beacon effects skipping spectators.
use crate::common::block_scenario::check;

#[test]
fn trapped_chest_opened_by_a_player_powers_dust_and_lamp() {
    check("blk_trapped_chest_open");
}
#[test]
fn opening_shulker_box_pops_torch_on_top_and_chest_sounds() {
    check("blk_container_open_effects");
}
#[test]
fn beacon_skips_spectators() {
    check("blk_beacon_spectator");
}
#[test]
fn creative_break_of_unopened_loot_shulker_drops_the_box() {
    check("blk_shulker_loot_creative");
}
