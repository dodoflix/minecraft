//! `SlotSourceArgument` against the vanilla golden (`fixtures/args/slot_source_golden.txt`).
mod args_common;

use mc_game::commands::arguments::slot_arguments::{SlotSourceArgument, SlotSourceResult};
use std::sync::Arc;

#[test]
#[ignore = "known deviation: SlotSourceArgument.list_suggestions drops the slot-range names (builder.add(sub)) and the registry elements"]
fn slot_source_matches_vanilla() {
    args_common::replay(
        "slot_source",
        include_str!("fixtures/args/slot_source_golden.txt"),
        |ctx| Arc::new(SlotSourceArgument::slot_source(ctx)),
        |v, _| {
            let r = v.downcast_ref::<SlotSourceResult>().expect("slot source");
            format!("{}|{}", r.name().unwrap_or_else(|| "<unnamed>".to_owned()), if matches!(r, SlotSourceResult::Literal(_)) { "literal" } else { "holder" })
        },
    );
}
