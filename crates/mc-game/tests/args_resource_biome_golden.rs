//! `ResourceArgument` over `minecraft:worldgen/biome` against the vanilla golden (`fixtures/args/resource_biome_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_biome_matches_vanilla() {
    args_common::replay(
        "resource_biome",
        include_str!("fixtures/args/resource_biome_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "minecraft:worldgen/biome")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
