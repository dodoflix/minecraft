//! `BlockStateArgument` against the vanilla golden (`fixtures/args/block_state_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn block_state_matches_vanilla() {
    args_common::replay_with(
        "block_state",
        include_str!("fixtures/args/block_state_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(blocks::block_state_argument::BlockStateArgument::block(ctx)) },
        |v, src| { let _ = src; let b = v.downcast_ref::<blocks::block_input::BlockInput>().expect("block"); let mut names: Vec<&str> = b.get_defined_properties().iter().map(|(p, _)| p.name).collect(); names.sort(); format!("{}|[{}]|{}", args_common::render_state(b.get_state()), names.join(", "), b.tag().map_or("null".to_owned(), |t| mc_nbt::Tag::Compound(t.clone()).to_string())) },
        None,
    );
}
