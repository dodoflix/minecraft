//! `ResourceArgument` over `minecraft:item` against the vanilla golden (`fixtures/args/resource_item_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_item_matches_vanilla() {
    args_common::replay(
        "resource_item",
        include_str!("fixtures/args/resource_item_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "item")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
