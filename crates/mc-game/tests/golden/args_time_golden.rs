//! `TimeArgument` against the vanilla golden (`fixtures/args/time_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::TimeArgument;
use std::sync::Arc;

#[test]
fn time_argument_matches_vanilla() {
    args_common::replay(
        "time",
        include_str!("../fixtures/args/time_golden.txt"),
        |_| Arc::new(TimeArgument::time()),
        |v, _| v.downcast_ref::<i32>().expect("int").to_string(),
    );
}
