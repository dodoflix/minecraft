//! `SwizzleArgument` against the vanilla golden (`fixtures/args/swizzle_golden.txt`, `scripts/args-golden.sh`).
extern crate mc_game_core as mc_game;
#[path = "../../mc-game/tests/args_common/mod.rs"]
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known limit: vanilla's message contains a lone surrogate (UTF-16 split of an emoji), not representable in a Rust String"]
fn swizzle_matches_vanilla() {
    args_common::replay(
        "swizzle",
        include_str!("../../mc-game/tests/fixtures/args/swizzle_golden.txt"),
        |_| Arc::new(coordinates::swizzle_argument::SwizzleArgument::swizzle()),
        |v, src| format!("[{}]", v.downcast_ref::<std::collections::BTreeSet<mc_core::direction::Axis>>().expect("axes").iter().map(|a| format!("{a:?}").to_lowercase()).collect::<Vec<_>>().join(", ")),
    );
}
