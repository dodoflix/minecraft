//! `IdentifierArgument` against the vanilla golden (`fixtures/args/identifier_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn identifier_matches_vanilla() {
    args_common::replay(
        "identifier",
        include_str!("../fixtures/args/identifier_golden.txt"),
        |_| Arc::new(IdentifierArgument::id()),
        |v, _src| v.downcast_ref::<mc_core::ident::Identifier>().expect("id").to_string(),
    );
}
