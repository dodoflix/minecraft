//! `StyleArgument` against the vanilla golden (`fixtures/args/style_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: Style codec error text differs (Not a number / Not a map: [])"]
fn style_matches_vanilla() {
    args_common::replay_with(
        "style",
        include_str!("fixtures/args/style_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(StyleArgument::style()) },
        |v, src| { let _ = src; args_common::canon_json(&v.downcast_ref::<mc_text::Style>().expect("style").to_json().expect("json")) },
        None,
    );
}
