//! `IntRangeArgument` against the vanilla golden (`fixtures/args/int_range_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn int_range_matches_vanilla() {
    args_common::replay(
        "int_range",
        include_str!("fixtures/args/int_range_golden.txt"),
        |_| Arc::new(IntRangeArgument::int_range()),
        |v, src| args_common::render_bounds(&v.downcast_ref::<min_max_bounds::Ints>().expect("ints").bounds, |x| x.to_string()),
    );
}
