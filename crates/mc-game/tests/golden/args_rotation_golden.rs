//! `RotationArgument` against the vanilla golden (`fixtures/args/rotation_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn rotation_matches_vanilla() {
    args_common::replay(
        "rotation",
        include_str!("../fixtures/args/rotation_golden.txt"),
        |_| Arc::new(coordinates::rotation_argument::RotationArgument::rotation()),
        |v, src| args_common::render_coordinates(v.downcast_ref::<coordinates::Coordinates>().expect("coordinates"), src),
    );
}
