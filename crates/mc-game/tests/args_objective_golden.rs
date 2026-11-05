//! `ObjectiveArgument` against the vanilla golden (`fixtures/args/objective_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn objective_matches_vanilla() {
    args_common::replay(
        "objective",
        include_str!("fixtures/args/objective_golden.txt"),
        |_| Arc::new(ObjectiveArgument::objective()),
        |v, src| v.downcast_ref::<String>().expect("name").clone(),
    );
}
