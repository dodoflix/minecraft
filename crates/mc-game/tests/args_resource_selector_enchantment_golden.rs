//! `ResourceSelectorArgument` over `minecraft:enchantment` against the vanilla golden (`fixtures/args/resource_selector_enchantment_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_selector_enchantment_matches_vanilla() {
    args_common::replay(
        "resource_selector_enchantment",
        include_str!("fixtures/args/resource_selector_enchantment_golden.txt"),
        |ctx| Arc::new(ResourceSelectorArgument::resource_selector(ctx, "minecraft:enchantment")),
        |v, _| args_common::render_id_list(&v.downcast_ref::<Vec<ResourceKey>>().expect("keys").iter().map(|k| k.id.to_string()).collect::<Vec<_>>()),
    );
}
