//! `CompoundTagArgument` against the vanilla golden (`fixtures/args/compound_tag_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn compound_tag_matches_vanilla() {
    args_common::replay_with(
        "compound_tag",
        include_str!("fixtures/args/compound_tag_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(CompoundTagArgument::compound_tag()) },
        |v, src| { let _ = src; mc_nbt::Tag::Compound(v.downcast_ref::<mc_nbt::CompoundTag>().expect("compound").clone()).to_string() },
        None,
    );
}
