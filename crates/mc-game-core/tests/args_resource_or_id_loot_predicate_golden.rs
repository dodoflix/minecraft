//! `ResourceOrIdArgument` (loot_predicate) against the vanilla golden (`fixtures/args/resource_or_id_loot_predicate_golden.txt`).
extern crate mc_game_core as mc_game;
#[path = "../../mc-game/tests/args_common/mod.rs"]
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: mc_dfu error text prints NBT values as JSON ({\"a\":1}, 1, 0.5) where vanilla prints SNBT ({a:1}, 1b, 0.5d) (feature: no Feature.DIRECT_CODEC yet)"]
fn resource_or_id_loot_predicate_matches_vanilla() {
    args_common::replay(
        "resource_or_id_loot_predicate",
        include_str!("../../mc-game/tests/fixtures/args/resource_or_id_loot_predicate_golden.txt"),
        |ctx| Arc::new(LootPredicateArgument::loot_predicate(ctx)),
        |v, _| args_common::render_resource_or_id(v.downcast_ref::<ResourceOrId>().expect("resource or id")),
    );
}
