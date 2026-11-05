//! `ResourceSelectorArgument` over `minecraft:entity_type` against the vanilla golden (`fixtures/args/resource_selector_entity_type_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_selector_entity_type_matches_vanilla() {
    args_common::replay(
        "resource_selector_entity_type",
        include_str!("fixtures/args/resource_selector_entity_type_golden.txt"),
        |ctx| Arc::new(ResourceSelectorArgument::resource_selector(ctx, "minecraft:entity_type")),
        |v, _| args_common::render_id_list(&v.downcast_ref::<Vec<ResourceKey>>().expect("keys").iter().map(|k| k.id.to_string()).collect::<Vec<_>>()),
    );
}
