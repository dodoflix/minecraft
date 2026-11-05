//! `NbtTagArgument` against the vanilla golden (`fixtures/args/nbt_tag_golden.txt`, `scripts/args-golden.sh`).
extern crate mc_game_core as mc_game;
#[path = "../../mc-game/tests/args_common/mod.rs"]
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: SNBT suggestions miss the word terminals (bool, uuid, true, false) after an unquoted-string failure"]
fn nbt_tag_matches_vanilla() {
    args_common::replay_with(
        "nbt_tag",
        include_str!("../../mc-game/tests/fixtures/args/nbt_tag_golden.txt"),
        |_ctx| { Arc::new(NbtTagArgument::nbt_tag()) },
        |v, _src| { v.downcast_ref::<mc_nbt::Tag>().expect("tag").to_string() },
        None,
    );
}
