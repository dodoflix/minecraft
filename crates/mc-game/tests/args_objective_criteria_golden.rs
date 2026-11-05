//! `ObjectiveCriteriaArgument` against the vanilla golden (`fixtures/args/objective_criteria_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn objective_criteria_matches_vanilla() {
    args_common::replay(
        "objective_criteria",
        include_str!("fixtures/args/objective_criteria_golden.txt"),
        |_| Arc::new(ObjectiveCriteriaArgument::criteria()),
        |v, src| args_common::render_criteria(v.downcast_ref::<scores::ObjectiveCriteria>().expect("criteria")),
    );
}
