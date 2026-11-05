//! `BlockPredicateArgument` against the vanilla golden (`fixtures/args/block_predicate_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn block_predicate_matches_vanilla() {
    args_common::replay_with(
        "block_predicate",
        include_str!("fixtures/args/block_predicate_golden.txt"),
        |ctx| { Arc::new(blocks::block_predicate_argument::BlockPredicateArgument::block_predicate(ctx)) },
        |v, _src| { args_common::render_block_predicate(v.downcast_ref::<blocks::block_predicate_argument::BlockPredicate>().expect("predicate")) },
        None,
    );
}
