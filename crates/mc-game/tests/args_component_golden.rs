//! `ComponentArgument` against the vanilla golden (`fixtures/args/component_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn component_matches_vanilla() {
    args_common::replay_with(
        "component",
        include_str!("fixtures/args/component_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(ComponentArgument::text_component()) },
        |v, src| { let _ = src; args_common::canon_json(&v.downcast_ref::<mc_text::Component>().expect("component").to_json().expect("json")) },
        None,
    );
}
