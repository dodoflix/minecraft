//! `SwizzleArgument` against the vanilla golden (`fixtures/args/swizzle_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn swizzle_matches_vanilla() {
    args_common::replay(
        "swizzle",
        include_str!("fixtures/args/swizzle_golden.txt"),
        |_| Arc::new(coordinates::swizzle_argument::SwizzleArgument::swizzle()),
        |v, src| format!("[{}]", v.downcast_ref::<std::collections::BTreeSet<mc_core::direction::Axis>>().expect("axes").iter().map(|a| format!("{a:?}").to_lowercase()).collect::<Vec<_>>().join(", ")),
    );
}
