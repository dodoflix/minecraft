//! Scaffolding collapse against the real vanilla level: scenario `tests/oracle/scenarios/blk_scaffolding_collapse.txt`,
//! fixture from `scripts/block-oracle.sh blk_scaffolding_collapse`.
use crate::common::block_scenario::check;

#[test]
fn scaffolding_collapse_matches_vanilla() {
    check("blk_scaffolding_collapse");
}
