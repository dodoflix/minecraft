//! `AngleArgument` against the vanilla golden (`fixtures/args/angle_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn angle_matches_vanilla() {
    args_common::replay(
        "angle",
        include_str!("fixtures/args/angle_golden.txt"),
        |_| Arc::new(AngleArgument::angle()),
        |v, _src| mc_core::math::java::java_float_to_string(v.downcast_ref::<SingleAngle>().expect("angle").get_angle(_src)),
    );
}
