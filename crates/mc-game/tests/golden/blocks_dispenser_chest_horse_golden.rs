//! Chests dispensed onto tamed chested horses against vanilla (`tests/oracle/scenarios/blk_disp_chest_horse.txt`; fixture
//! regenerated with `scripts/block-oracle.sh blk_disp_chest_horse`).

use crate::blocks_dispenser_fire_golden::check;

#[test]
fn chest_onto_tamed_chested_horse() {
    check("blk_disp_chest_horse");
}
