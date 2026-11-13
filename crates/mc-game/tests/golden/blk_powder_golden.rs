//! Concrete powder, powder snow and magma against the real vanilla level
//! (`tests/oracle/scenarios/blk_fluid_{concrete_powder,powder_snow}.txt`, `blk_sl_magma.txt`; fixtures from
//! `scripts/block-oracle.sh NAME`).
use crate::common::block_scenario::{check, check_with};
use crate::common::entity_oracle::Options;

#[test]
fn concrete_powder_solidifies_like_vanilla() {
    check("blk_fluid_concrete_powder");
}

#[test]
fn powder_snow_entities_fall_sounds_and_fire() {
    check("blk_fluid_powder_snow");
}

#[test]
fn magma_block_hurts_living_entities_standing_on_it() {
    // the magma replay compares the entity lines, not the sounds
    check_with(
        "blk_sl_magma",
        &Options {
            ignore_kinds: vec!["EV"],
            ..Options::default()
        },
    );
}
