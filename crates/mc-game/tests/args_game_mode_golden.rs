//! `GameModeArgument` against the vanilla golden (`fixtures/args/game_mode_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn game_mode_matches_vanilla() {
    args_common::replay(
        "game_mode",
        include_str!("fixtures/args/game_mode_golden.txt"),
        |_| Arc::new(GameModeArgument::game_mode()),
        |v, _src| v.downcast_ref::<mc_world::level_storage::GameType>().expect("gm").get_name().to_owned(),
    );
}
