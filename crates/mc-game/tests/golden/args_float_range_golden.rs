//! `FloatRangeArgument` against the vanilla golden (`fixtures/args/float_range_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn float_range_matches_vanilla() {
    args_common::replay(
        "float_range",
        include_str!("../fixtures/args/float_range_golden.txt"),
        |_| Arc::new(FloatRangeArgument::float_range()),
        |v, src| args_common::render_doubles(v.downcast_ref::<min_max_bounds::Doubles>().expect("doubles")),
    );
}
