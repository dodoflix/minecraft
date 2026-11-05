//! `ScoreboardSlotArgument` against the vanilla golden (`fixtures/args/scoreboard_slot_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn scoreboard_slot_matches_vanilla() {
    args_common::replay(
        "scoreboard_slot",
        include_str!("../fixtures/args/scoreboard_slot_golden.txt"),
        |_| Arc::new(ScoreboardSlotArgument::display_slot()),
        |v, src| v.downcast_ref::<scores::DisplaySlot>().expect("slot").get_serialized_name().to_owned(),
    );
}
