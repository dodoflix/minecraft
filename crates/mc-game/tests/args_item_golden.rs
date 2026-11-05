//! `ItemArgument` against the vanilla golden (`fixtures/args/item_golden.txt`, `scripts/args-golden.sh`).
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn item_matches_vanilla() {
    args_common::replay_with(
        "item",
        include_str!("fixtures/args/item_golden.txt"),
        |ctx| { let _ = ctx; Arc::new(item::item_argument::ItemArgument::item(ctx)) },
        |v, src| { let _ = src; let i = v.downcast_ref::<item::item_input::ItemInput>().expect("item"); let patch = mc_dfu::encode_nbt(&mc_protocol::components::DataComponentPatch::codec(), &i.components).into_result().map_or("<encode failed>".to_owned(), |t| t.to_string()); format!("{}|{patch}", i.item.name()) },
        None,
    );
}
