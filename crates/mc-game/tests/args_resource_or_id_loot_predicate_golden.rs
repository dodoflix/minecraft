//! `ResourceOrIdArgument` (loot_predicate) against the vanilla golden (`fixtures/args/resource_or_id_loot_predicate_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_or_id_loot_predicate_matches_vanilla() {
    args_common::replay(
        "resource_or_id_loot_predicate",
        include_str!("fixtures/args/resource_or_id_loot_predicate_golden.txt"),
        |ctx| Arc::new(LootPredicateArgument::loot_predicate(ctx)),
        |v, _| args_common::render_resource_or_id(v.downcast_ref::<ResourceOrId>().expect("resource or id")),
    );
}
