//! `compute`, `data modify ... compute` and the `predicate` selector option against the real vanilla level: loot contexts built by
//! the command layer. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`.

use crate::common;

#[test]
fn compute_values_match_vanilla() {
    common::block_scenario::check("cmd_compute_values");
}

#[test]
fn compute_context_sources_match_vanilla() {
    common::block_scenario::check("cmd_compute_sources");
}

#[test]
fn data_modify_compute_matches_vanilla() {
    common::block_scenario::check("cmd_data_modify_compute");
}

#[test]
fn selector_predicate_option_matches_vanilla() {
    common::block_scenario::check("cmd_select_predicate");
}
