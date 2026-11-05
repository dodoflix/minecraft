//! `Vec2Argument` against the vanilla golden (`fixtures/args/vec2_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use mc_game::commands::arguments::coordinates::vec2_argument::Vec2Argument;
use std::sync::Arc;

#[test]
fn vec2_matches_vanilla() {
    args_common::replay(
        "vec2",
        include_str!("fixtures/args/vec2_golden.txt"),
        |_| Arc::new(Vec2Argument::vec2()),
        |v, src| args_common::render_coordinates(v.downcast_ref::<coordinates::Coordinates>().expect("coordinates"), src),
    );
}
