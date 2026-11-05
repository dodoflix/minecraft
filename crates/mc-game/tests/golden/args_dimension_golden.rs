//! `DimensionArgument` against the vanilla golden (`fixtures/args/dimension_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn dimension_matches_vanilla() {
    args_common::replay(
        "dimension",
        include_str!("../fixtures/args/dimension_golden.txt"),
        |_| Arc::new(DimensionArgument::dimension()),
        |v, _src| v.downcast_ref::<mc_core::ident::Identifier>().expect("id").to_string(),
    );
}
