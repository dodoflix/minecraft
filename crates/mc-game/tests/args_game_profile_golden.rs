//! `GameProfileArgument` against the vanilla golden (`fixtures/args/game_profile_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn game_profile_matches_vanilla() {
    args_common::replay_with(
        "game_profile",
        include_str!("fixtures/args/game_profile_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(GameProfileArgument::game_profile()) },
        |v, src| { let _ = src; match v.downcast_ref::<game_profile_argument::GameProfileResult>().expect("result") { game_profile_argument::GameProfileResult::Selector(s) => format!("selector {}", s.describe()), _ => "plain".to_owned() } },
        None,
    );
}
