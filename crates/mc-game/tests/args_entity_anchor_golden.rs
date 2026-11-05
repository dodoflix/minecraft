//! `EntityAnchorArgument` against the vanilla golden (`fixtures/args/entity_anchor_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use mc_game::commands::arguments::entity_anchor_argument::EntityAnchorArgument;
use std::sync::Arc;

#[test]
fn entity_anchor_matches_vanilla() {
    args_common::replay(
        "entity_anchor",
        include_str!("fixtures/args/entity_anchor_golden.txt"),
        |_| Arc::new(EntityAnchorArgument::anchor()),
        |v, _src| format!("{:?}", v.downcast_ref::<mc_game::commands::arguments::entity_anchor_argument::Anchor>().expect("a")).to_uppercase(),
    );
}
