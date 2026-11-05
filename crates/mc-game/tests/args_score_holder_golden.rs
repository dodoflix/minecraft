//! `ScoreHolderArgument::score_holder()` (with `SUGGEST_SCORE_HOLDERS`) against the vanilla golden (`fixtures/args/score_holder_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn score_holder_matches_vanilla() {
    args_common::replay_with(
        "score_holder",
        include_str!("fixtures/args/score_holder_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(ScoreHolderArgument::score_holder()) },
        |v, src| { let _ = src; args_common::render_score_holder(v.downcast_ref::<scores::ScoreHolderResult>().expect("result"), &args_common::render_source()) },
        Some(scores::suggest_score_holders()),
    );
}
