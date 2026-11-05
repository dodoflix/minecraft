//! `Vec3Argument` against the vanilla golden (`fixtures/args/vec3_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use mc_game::commands::arguments::coordinates::vec3_argument::Vec3Argument;
use std::sync::Arc;

#[test]
fn vec3_matches_vanilla() {
    args_common::replay(
        "vec3",
        include_str!("../fixtures/args/vec3_golden.txt"),
        |_| Arc::new(Vec3Argument::vec3()),
        |v, src| args_common::render_coordinates(v.downcast_ref::<coordinates::Coordinates>().expect("coordinates"), src),
    );
}
