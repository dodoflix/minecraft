//! Tests that need a process of their own (T0/T1): the log capture is process-wide, so tests that assert on log lines run apart from the rest.

extern crate mc_game_core as mc_game;

#[path = "../../../mc-game/tests/golden/common/mod.rs"]
mod common;

mod gen_level_semantics;
