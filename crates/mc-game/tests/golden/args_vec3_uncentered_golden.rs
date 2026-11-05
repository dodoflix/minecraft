//! `Vec3Argument` against the vanilla golden (`fixtures/args/vec3_uncentered_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use mc_game::commands::arguments::coordinates::vec3_argument::Vec3Argument;
use std::sync::Arc;

#[test]
fn vec3_uncentered_matches_vanilla() {
    args_common::replay(
        "vec3_uncentered",
        include_str!("../fixtures/args/vec3_uncentered_golden.txt"),
        |_| Arc::new(Vec3Argument::new(false)),
        |v, src| args_common::render_coordinates(v.downcast_ref::<coordinates::Coordinates>().expect("coordinates"), src),
    );
}
