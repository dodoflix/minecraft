//! `TeamArgument` against the vanilla golden (`fixtures/args/team_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn team_matches_vanilla() {
    args_common::replay(
        "team",
        include_str!("../fixtures/args/team_golden.txt"),
        |_| Arc::new(TeamArgument::team()),
        |v, src| v.downcast_ref::<String>().expect("name").clone(),
    );
}
