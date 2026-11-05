//! `ResourceArgument` over `minecraft:entity_type` against the vanilla golden (`fixtures/args/resource_entity_type_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_entity_type_matches_vanilla() {
    args_common::replay(
        "resource_entity_type",
        include_str!("fixtures/args/resource_entity_type_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "minecraft:entity_type")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
