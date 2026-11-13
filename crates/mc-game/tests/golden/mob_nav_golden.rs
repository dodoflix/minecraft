//! Navigation recomputation against vanilla: a block that changes the collision shape near a path makes the navigation recompute it
//! (delayed while the last recomputation is under 20 ticks old), and `/tp` stops the navigation of a path finding mob.
//! Scenarios: `tests/oracle/scenarios/mob_{nav_block_update,tp_stops_nav}.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh NAME`). The block replay runs them: its chunks tick, so `setBlock` reaches `sendBlockUpdated`.
use crate::common::block_scenario::check;

#[test]
fn nav_block_update_matches_vanilla() {
    check("mob_nav_block_update");
}

#[test]
fn tp_stops_nav_matches_vanilla() {
    check("mob_tp_stops_nav");
}
