//! `TeamColorArgument` against the vanilla golden (`fixtures/args/team_color_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn team_color_matches_vanilla() {
    args_common::replay(
        "team_color",
        include_str!("../fixtures/args/team_color_golden.txt"),
        |_| Arc::new(TeamColorArgument::team_color()),
        |v, _src| v.downcast_ref::<TeamColor>().expect("c").get_serialized_name().to_owned(),
    );
}
