//! The dispenser behaviours that act on blocks and block entities against vanilla: glass bottles, shears, glowstone, wither
//! skeleton skulls and carved pumpkins (`tests/oracle/scenarios/blk_disp_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh blk_disp_NAME`).

use crate::blocks_dispenser_spine_golden::check;

#[test]
fn bottles() {
    check("blk_disp_bottles");
}

#[test]
fn shears() {
    check("blk_disp_shears");
}

#[test]
fn shears_on_bogged_and_sulfur_cube() {
    check("blk_disp_shears_shearable_mobs");
}

#[test]
fn glowstone() {
    check("blk_disp_glowstone");
}

#[test]
fn skulls() {
    check("blk_disp_skulls");
}

#[test]
fn pumpkin() {
    check("blk_disp_pumpkin");
}
