//! `TimeArgument` against the vanilla golden (`fixtures/args/time_min_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn time_min_matches_vanilla() {
    args_common::replay(
        "time_min",
        include_str!("fixtures/args/time_min_golden.txt"),
        |_| Arc::new(TimeArgument::time_min(20)),
        |v, _src| v.downcast_ref::<i32>().expect("int").to_string(),
    );
}
