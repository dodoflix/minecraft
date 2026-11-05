//! `ResourceKeyArgument` over `minecraft:block` against the vanilla golden (`fixtures/args/resource_key_block_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_key_block_matches_vanilla() {
    args_common::replay(
        "resource_key_block",
        include_str!("../fixtures/args/resource_key_block_golden.txt"),
        |_| Arc::new(ResourceKeyArgument::key("block")),
        |v, _| args_common::render_key(v.downcast_ref::<ResourceKey>().expect("key")),
    );
}
