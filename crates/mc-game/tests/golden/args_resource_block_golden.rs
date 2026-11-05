//! `ResourceArgument` over `minecraft:block` against the vanilla golden (`fixtures/args/resource_block_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_block_matches_vanilla() {
    args_common::replay(
        "resource_block",
        include_str!("../fixtures/args/resource_block_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "block")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
