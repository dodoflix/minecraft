//! `ResourceArgument` over `minecraft:mob_effect` against the vanilla golden (`fixtures/args/resource_mob_effect_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_mob_effect_matches_vanilla() {
    args_common::replay(
        "resource_mob_effect",
        include_str!("fixtures/args/resource_mob_effect_golden.txt"),
        |ctx| Arc::new(ResourceArgument::resource(ctx, "mob_effect")),
        |v, _| v.downcast_ref::<ResourceKey>().expect("key").id.to_string(),
    );
}
