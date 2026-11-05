//! `MessageArgument` against the vanilla golden (`fixtures/args/message_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn message_matches_vanilla() {
    args_common::replay_with(
        "message",
        include_str!("fixtures/args/message_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(MessageArgument::message()) },
        |v, src| { let _ = src; args_common::render_message(v.downcast_ref::<message_argument::Message>().expect("message")) },
        None,
    );
}
