//! `ResourceArgument` over `minecraft:worldgen/structure` against the vanilla golden (`fixtures/args/resource_structure_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_structure_matches_vanilla() {
    args_common::replay(
        "resource_structure",
        include_str!("../fixtures/args/resource_structure_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "worldgen/structure")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
