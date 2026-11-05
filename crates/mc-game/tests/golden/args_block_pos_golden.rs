//! `BlockPosArgument` against the vanilla golden (`fixtures/args/block_pos_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn block_pos_matches_vanilla() {
    args_common::replay(
        "block_pos",
        include_str!("../fixtures/args/block_pos_golden.txt"),
        |_| Arc::new(coordinates::block_pos_argument::BlockPosArgument::block_pos()),
        |v, src| args_common::render_coordinates(v.downcast_ref::<coordinates::Coordinates>().expect("coordinates"), src),
    );
}
