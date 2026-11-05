//! `ResourceKeyArgument` over `minecraft:worldgen/biome` against the vanilla golden (`fixtures/args/resource_key_biome_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_key_biome_matches_vanilla() {
    args_common::replay(
        "resource_key_biome",
        include_str!("fixtures/args/resource_key_biome_golden.txt"),
        |_| Arc::new(ResourceKeyArgument::key("worldgen/biome")),
        |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
    );
}
