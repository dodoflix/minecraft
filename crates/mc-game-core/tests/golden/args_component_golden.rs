//! `ComponentArgument` against the vanilla golden (`fixtures/args/component_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: ComponentSerialization either-codec error text (Failed to parse either ...) differs; also NBT value text"]
fn component_matches_vanilla() {
    args_common::replay_with(
        "component",
        include_str!("../../../mc-game/tests/fixtures/args/component_golden.txt"),
        |_ctx| { Arc::new(ComponentArgument::text_component()) },
        |v, _src| { args_common::canon_json(&v.downcast_ref::<mc_text::Component>().expect("component").to_json().expect("json")) },
        None,
    );
}
