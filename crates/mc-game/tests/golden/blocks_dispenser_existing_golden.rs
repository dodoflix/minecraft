//! The dispense behaviours of armor stands, boats, projectiles, fire and wind charges, water bottles, shulker boxes and TNT, and the projectiles
//! of the ominous item spawner, against vanilla (`tests/oracle/scenarios/blk_disp_*.txt`, `blk_ominous_item_projectile.txt`;
//! fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn armor_stand() {
    common::block_scenario::check("blk_disp_armor_stand");
}

#[test]
fn boat() {
    common::block_scenario::check("blk_disp_boat");
}

#[test]
fn projectiles() {
    common::block_scenario::check("blk_disp_projectiles");
}

#[test]
fn projectiles_take_the_can_break_component() {
    common::block_scenario::check("blk_disp_projectile_can_break");
}

#[test]
fn charges() {
    common::block_scenario::check("blk_disp_charges");
}

#[test]
fn ominous_item_projectiles() {
    common::block_scenario::check("blk_ominous_item_projectile");
}

#[test]
fn water_bottle() {
    common::block_scenario::check("blk_disp_water_bottle");
}

#[test]
fn shulker_box() {
    common::block_scenario::check("blk_disp_shulker");
}

#[test]
fn tnt() {
    common::block_scenario::check("blk_disp_tnt");
}
