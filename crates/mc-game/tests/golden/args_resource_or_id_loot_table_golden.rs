//! `ResourceOrIdArgument` (loot_table) against the vanilla golden (`fixtures/args/resource_or_id_loot_table_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_or_id_loot_table_matches_vanilla() {
    args_common::replay(
        "resource_or_id_loot_table",
        include_str!("../fixtures/args/resource_or_id_loot_table_golden.txt"),
        |ctx| Arc::new(LootTableArgument::loot_table(ctx)),
        |v, _| args_common::render_resource_or_id(v.downcast_ref::<ResourceOrId>().expect("resource or id")),
    );
}
