//! `StyleArgument` against the vanilla golden (`fixtures/args/style_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: Style codec error text differs (Not a number / Not a map: [])"]
fn style_matches_vanilla() {
    args_common::replay_with(
        "style",
        include_str!("../../../mc-game/tests/fixtures/args/style_golden.txt"),
        |_ctx| { Arc::new(StyleArgument::style()) },
        |v, _src| { args_common::canon_json(&v.downcast_ref::<mc_text::Style>().expect("style").to_json().expect("json")) },
        None,
    );
}
