//! `HexColorArgument` against the vanilla golden (`fixtures/args/hex_color_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn hex_color_matches_vanilla() {
    args_common::replay(
        "hex_color",
        include_str!("../fixtures/args/hex_color_golden.txt"),
        |_| Arc::new(HexColorArgument::hex_color()),
        |v, _| v.downcast_ref::<i32>().expect("int").to_string(),
    );
}
