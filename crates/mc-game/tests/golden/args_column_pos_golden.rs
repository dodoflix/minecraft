//! `ColumnPosArgument` against the vanilla golden (`fixtures/args/column_pos_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn column_pos_matches_vanilla() {
    args_common::replay(
        "column_pos",
        include_str!("../fixtures/args/column_pos_golden.txt"),
        |_| Arc::new(coordinates::column_pos_argument::ColumnPosArgument::column_pos()),
        |v, src| args_common::render_coordinates(v.downcast_ref::<coordinates::Coordinates>().expect("coordinates"), src),
    );
}
