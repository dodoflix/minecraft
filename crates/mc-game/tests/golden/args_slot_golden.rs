//! `SlotArgument` against the vanilla golden (`fixtures/args/slot_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn slot_matches_vanilla() {
    args_common::replay_with(
        "slot",
        include_str!("../fixtures/args/slot_golden.txt"),
        |_ctx| { Arc::new(slot_arguments::SlotArgument::slot()) },
        |v, _src| { v.downcast_ref::<i32>().expect("slot").to_string() },
        None,
    );
}
