//! Replays the committed entity-oracle scenarios through the generic scenario harness (`common/entity_scenario.rs`) and compares
//! them with the vanilla fixtures. Known gaps of the Rust server are listed as `ignore_fields` per scenario.
mod common;
use common::entity_oracle::Options;
use common::entity_scenario::*;

#[test]
fn fall() {
    run_scenario_test("fall");
}
