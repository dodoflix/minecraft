//! The dispense behaviours of armor stands, boats, water bottles, shulker boxes and TNT against vanilla
//! (`tests/oracle/scenarios/blk_disp_*.txt`; fixtures regenerated with `scripts/block-oracle.sh blk_disp_NAME`).

use crate::common;
use crate::common::entity_oracle::Options;

#[test]
fn armor_stand() {
    common::block_scenario::check("blk_disp_armor_stand");
}

#[test]
fn boat() {
    // Not implemented in the scenario replay: the boat fields of the oracle's `E` lines (the boat goldens compare them)
    common::block_scenario::check_with(
        "blk_disp_boat",
        &Options {
            ignore_fields: vec!["status", "dmg", "hurt", "bubble", "paddle", "pass", "items"],
            ..Default::default()
        },
    );
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
