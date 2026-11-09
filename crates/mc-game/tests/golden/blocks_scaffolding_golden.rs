//! Scaffolding collapse and placement against the real vanilla level: scenarios `tests/oracle/scenarios/blk_scaffolding_*.txt`,
//! fixtures from `scripts/block-oracle.sh blk_scaffolding_collapse blk_scaffolding_place`.
use crate::common::block_scenario::check;

#[test]
fn scaffolding_collapse_matches_vanilla() {
    check("blk_scaffolding_collapse");
}

#[test]
fn scaffolding_item_places_at_the_end_of_the_line() {
    check("blk_scaffolding_place");
}
