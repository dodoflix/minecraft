//! `ResourceOrIdArgument` (int_provider) against the vanilla golden (`fixtures/args/resource_or_id_int_provider_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_or_id_int_provider_matches_vanilla() {
    args_common::replay(
        "resource_or_id_int_provider",
        include_str!("fixtures/args/resource_or_id_int_provider_golden.txt"),
        |ctx| Arc::new(ContextIntProviderArgument::int_provider(ctx)),
        |v, _| args_common::render_resource_or_id(v.downcast_ref::<ResourceOrId>().expect("resource or id")),
    );
}
