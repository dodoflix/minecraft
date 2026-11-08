//! Furnace, smoker and blast furnace goldens: scenarios `tests/oracle/scenarios/blk_furnace_*.txt` replayed on the real
//! vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_furnace_*.txt`).

use crate::common;

/// Every cooking recipe of each furnace type plus the edge cases of the cooking tick (`gen_furnace_scenarios.py`). The
/// bucket the lava bucket stack leaves behind rests for 500 ticks: its `delta` depends on the entity id (the motion tick
/// parity), which the replay does not number like the vanilla level.
#[test]
fn cooking_matches_vanilla() {
    common::block_scenario::check_with(
        "blk_furnace_cook",
        &common::entity_oracle::Options {
            ignore_fields: vec!["delta"],
            ..Default::default()
        },
    );
}

/// Every distinct cooking fuel in each furnace type (`gen_furnace_scenarios.py`).
#[test]
fn fuels_match_vanilla() {
    common::block_scenario::check("blk_furnace_fuels");
}

#[test]
fn recipes_used_and_experience_match_vanilla() {
    common::block_scenario::check("blk_furnace_xp_recipes_used");
}

#[test]
fn hopper_transfers_match_vanilla() {
    common::block_scenario::check("blk_furnace_hopper");
}

#[test]
fn removal_drops_and_comparators_match_vanilla() {
    common::block_scenario::check("blk_furnace_break_drops");
}

#[test]
fn placement_facing_matches_vanilla() {
    common::block_scenario::check("blk_furnace_place");
}
