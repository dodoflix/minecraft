//! `ResourceArgument` over `minecraft:attribute` against the vanilla golden (`fixtures/args/resource_attribute_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_attribute_matches_vanilla() {
    args_common::replay(
        "resource_attribute",
        include_str!("fixtures/args/resource_attribute_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "attribute")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
