//! `ResourceArgument` over `minecraft:enchantment` against the vanilla golden (`fixtures/args/resource_enchantment_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_enchantment_matches_vanilla() {
    args_common::replay(
        "resource_enchantment",
        include_str!("fixtures/args/resource_enchantment_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "minecraft:enchantment")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
