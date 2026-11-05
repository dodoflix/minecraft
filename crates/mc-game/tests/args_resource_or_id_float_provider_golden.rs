//! `ResourceOrIdArgument` (float_provider) against the vanilla golden (`fixtures/args/resource_or_id_float_provider_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: mc_dfu error text prints NBT values as JSON ({\"a\":1}, 1, 0.5) where vanilla prints SNBT ({a:1}, 1b, 0.5d) (feature: no Feature.DIRECT_CODEC yet)"]
fn resource_or_id_float_provider_matches_vanilla() {
    args_common::replay(
        "resource_or_id_float_provider",
        include_str!("fixtures/args/resource_or_id_float_provider_golden.txt"),
        |ctx| Arc::new(ContextFloatProviderArgument::float_provider(ctx)),
        |v, _| args_common::render_resource_or_id(v.downcast_ref::<ResourceOrId>().expect("resource or id")),
    );
}
