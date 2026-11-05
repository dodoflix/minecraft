//! `ResourceKeyArgument` over `minecraft:enchantment` against the vanilla golden (`fixtures/args/resource_key_enchantment_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_key_enchantment_matches_vanilla() {
    args_common::replay(
        "resource_key_enchantment",
        include_str!("fixtures/args/resource_key_enchantment_golden.txt"),
        |_| Arc::new(ResourceKeyArgument::key("enchantment")),
        |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
    );
}
