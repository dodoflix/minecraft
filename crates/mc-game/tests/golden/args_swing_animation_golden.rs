//! `SwingAnimationArgument` against the vanilla golden (`fixtures/args/swing_animation_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn swing_animation_matches_vanilla() {
    args_common::replay(
        "swing_animation",
        include_str!("../fixtures/args/swing_animation_golden.txt"),
        |_| Arc::new(SwingAnimationArgument::swing_animation_type()),
        |v, _src| v.downcast_ref::<SwingAnimationType>().expect("s").get_serialized_name().to_owned(),
    );
}
