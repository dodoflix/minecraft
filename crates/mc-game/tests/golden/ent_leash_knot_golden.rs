//! Leash knots and leads on fences against vanilla (`tests/oracle/scenarios/*.txt`, fixtures from `scripts/entity-oracle.sh`).

use crate::common::entity_scenario::run_scenario_test;

#[test]
fn leash_knot_matches_vanilla() {
    run_scenario_test("ent_leash_knot");
}

#[test]
fn lead_on_a_fence_matches_vanilla() {
    run_scenario_test("itm_lead_fence");
}

#[test]
fn leash_position_tag_restores_the_knot() {
    run_scenario_test("ent_leash_restore");
}

#[test]
fn leash_knot_packets_and_save_data_match_vanilla() {
    crate::common::block_scenario::check("ent_knot_packets");
}
