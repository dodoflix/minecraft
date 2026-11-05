//! `ResourceKeyArgument` over `minecraft:mob_effect` against the vanilla golden (`fixtures/args/resource_key_mob_effect_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_key_mob_effect_matches_vanilla() {
    args_common::replay(
        "resource_key_mob_effect",
        include_str!("fixtures/args/resource_key_mob_effect_golden.txt"),
        |_| Arc::new(ResourceKeyArgument::key("mob_effect")),
        |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
    );
}
