//! `MessageArgument` without selector permission against the vanilla golden (`fixtures/args/message_noselectors_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn message_noselectors_matches_vanilla() {
    args_common::replay_with(
        "message_noselectors",
        include_str!("../fixtures/args/message_noselectors_golden.txt"),
        |_ctx| { Arc::new(MessageArgument::message()) },
        |v, _src| { args_common::render_message(v.downcast_ref::<message_argument::Message>().expect("message")) },
        None,
    );
}
