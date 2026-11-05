//! `ScoreHolderArgument::score_holders()` (with `SUGGEST_SCORE_HOLDERS`) against the vanilla golden (`fixtures/args/score_holders_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn score_holders_matches_vanilla() {
    args_common::replay_with(
        "score_holders",
        include_str!("fixtures/args/score_holders_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(ScoreHolderArgument::score_holders()) },
        |v, src| { let _ = src; args_common::render_score_holder(v.downcast_ref::<scores::ScoreHolderResult>().expect("result"), &args_common::render_source()) },
        Some(scores::suggest_score_holders()),
    );
}
