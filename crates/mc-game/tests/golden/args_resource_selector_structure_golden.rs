//! `ResourceSelectorArgument` over `minecraft:worldgen/structure` against the vanilla golden (`fixtures/args/resource_selector_structure_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_selector_structure_matches_vanilla() {
    args_common::replay(
        "resource_selector_structure",
        include_str!("../fixtures/args/resource_selector_structure_golden.txt"),
        |ctx| Arc::new(ResourceSelectorArgument::resource_selector(ctx, "worldgen/structure")),
        |v, _| args_common::render_id_list(&v.downcast_ref::<Vec<ResourceKey>>().expect("keys").iter().map(|k| k.id.to_string()).collect::<Vec<_>>()),
    );
}
