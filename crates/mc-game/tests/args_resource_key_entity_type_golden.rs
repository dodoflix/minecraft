//! `ResourceKeyArgument` over `minecraft:entity_type` against the vanilla golden (`fixtures/args/resource_key_entity_type_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_key_entity_type_matches_vanilla() {
    args_common::replay(
        "resource_key_entity_type",
        include_str!("fixtures/args/resource_key_entity_type_golden.txt"),
        |_| Arc::new(ResourceKeyArgument::key("minecraft:entity_type")),
        |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
    );
}
