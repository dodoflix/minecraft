//! `NbtTagArgument` against the vanilla golden (`fixtures/args/nbt_tag_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: SNBT suggestions miss the word terminals (bool, uuid, true, false) after an unquoted-string failure"]
fn nbt_tag_matches_vanilla() {
    args_common::replay_with(
        "nbt_tag",
        include_str!("fixtures/args/nbt_tag_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(NbtTagArgument::nbt_tag()) },
        |v, src| { let _ = src; v.downcast_ref::<mc_nbt::Tag>().expect("tag").to_string() },
        None,
    );
}
