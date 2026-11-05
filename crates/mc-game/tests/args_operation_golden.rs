//! `OperationArgument` against the vanilla golden (`fixtures/args/operation_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn operation_matches_vanilla() {
    args_common::replay(
        "operation",
        include_str!("fixtures/args/operation_golden.txt"),
        |_| Arc::new(OperationArgument::operation()),
        |v, src| args_common::render_operation(*v.downcast_ref::<Operation>().expect("operation")),
    );
}
