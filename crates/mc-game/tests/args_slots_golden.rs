//! `SlotsArgument` against the vanilla golden (`fixtures/args/slots_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn slots_matches_vanilla() {
    args_common::replay_with(
        "slots",
        include_str!("fixtures/args/slots_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(slot_arguments::SlotsArgument::slots()) },
        |v, src| { let _ = src; let r = v.downcast_ref::<slot_arguments::SlotRange>().expect("range"); format!("{}|[{}]", r.get_serialized_name(), r.slots().iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ")) },
        None,
    );
}
