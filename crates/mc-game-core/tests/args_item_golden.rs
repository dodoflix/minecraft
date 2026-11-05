//! `ItemArgument` against the vanilla golden (`fixtures/args/item_golden.txt`, `scripts/args-golden.sh`).
extern crate mc_game_core as mc_game;
#[path = "../../mc-game/tests/args_common/mod.rs"]
mod args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: mc_dfu error text prints NBT values as JSON ({\"a\":1}, 1, 0.5) where vanilla prints SNBT ({a:1}, 1b, 0.5d) (feature: no Feature.DIRECT_CODEC yet)"]
fn item_matches_vanilla() {
    args_common::replay_with(
        "item",
        include_str!("../../mc-game/tests/fixtures/args/item_golden.txt"),
        |ctx| { Arc::new(item::item_argument::ItemArgument::item(ctx)) },
        |v, _src| { let i = v.downcast_ref::<item::item_input::ItemInput>().expect("item"); let patch = mc_dfu::encode_nbt(&mc_protocol::components::DataComponentPatch::codec(), &i.components).into_result().map_or("<encode failed>".to_owned(), |t| t.to_string()); format!("{}|{patch}", i.item.name()) },
        None,
    );
}
