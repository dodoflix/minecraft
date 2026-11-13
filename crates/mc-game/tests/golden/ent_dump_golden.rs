//! The rich entity dump (`xentities`) and the synched data dump (`edata:`) of every non-mob entity class, in both replays
//! (`tests/oracle/scenarios/ent_dump_smoke.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn every_class_dumps_like_vanilla() {
    common::block_scenario::check("ent_dump_smoke");
}

#[test]
fn entity_replay_dumps_like_vanilla() {
    common::entity_scenario::run_scenario_test("ent_dump_smoke");
}
