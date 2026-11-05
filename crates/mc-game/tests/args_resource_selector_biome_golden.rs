//! `ResourceSelectorArgument` over `minecraft:worldgen/biome` against the vanilla golden (`fixtures/args/resource_selector_biome_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_selector_biome_matches_vanilla() {
    args_common::replay(
        "resource_selector_biome",
        include_str!("fixtures/args/resource_selector_biome_golden.txt"),
        |ctx| Arc::new(ResourceSelectorArgument::resource_selector(ctx, "worldgen/biome")),
        |v, _| args_common::render_id_list(&v.downcast_ref::<Vec<ResourceKey>>().expect("keys").iter().map(|k| k.id.to_string()).collect::<Vec<_>>()),
    );
}
