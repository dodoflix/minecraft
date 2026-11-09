//! Concrete powder, powder snow and magma against the real vanilla level
//! (`tests/oracle/scenarios/blk_fluid_{concrete_powder,powder_snow}.txt`, `blk_sl_magma.txt`; fixtures from
//! `scripts/block-oracle.sh NAME`).
use crate::common::block_scenario::{check, check_with};
use crate::common::entity_oracle::Options;

/// The mobs' body rotation and ambient sounds follow AI random draws that these scenarios do not test (`ignore_containing`
/// drops the ambient sounds of the powder snow mobs; the magma replay compares the entity lines, not the sounds).
fn check_ignoring_mob_ai(name: &str, options: Options) {
    check_with(
        name,
        &Options {
            ignore_fields: vec!["rot"],
            ..options
        },
    );
}

#[test]
fn concrete_powder_solidifies_like_vanilla() {
    check("blk_fluid_concrete_powder");
}

#[test]
fn powder_snow_entities_fall_sounds_and_fire() {
    check_ignoring_mob_ai(
        "blk_fluid_powder_snow",
        Options {
            ignore_containing: vec![".ambient"],
            ..Options::default()
        },
    );
}

#[test]
fn magma_block_hurts_living_entities_standing_on_it() {
    check_ignoring_mob_ai(
        "blk_sl_magma",
        Options {
            ignore_kinds: vec!["EV"],
            ..Options::default()
        },
    );
}
