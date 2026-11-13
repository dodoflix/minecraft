//! The armadillo against vanilla: scare sensor and roll-up states, scute shedding, brushing by hand and by dispenser
//! (`tests/oracle/scenarios/{mob_armadillo_*,itm_brush_armadillo,blk_disp_brush_armadillo}.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::blocks_dispenser_spine_golden::check;

#[test]
fn scare_roll() {
    check("mob_armadillo_scare_roll");
}

#[test]
fn scute_drop() {
    check("mob_armadillo_scute_drop");
}

#[test]
fn brush_by_hand() {
    check("itm_brush_armadillo");
}

#[test]
fn brush_by_dispenser() {
    check("blk_disp_brush_armadillo");
}
