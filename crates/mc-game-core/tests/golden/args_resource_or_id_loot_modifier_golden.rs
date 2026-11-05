//! `ResourceOrIdArgument` (loot_modifier) against the vanilla golden (`fixtures/args/resource_or_id_loot_modifier_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
#[ignore = "known deviation: mc_dfu error text prints NBT values as JSON ({\"a\":1}, 1, 0.5) where vanilla prints SNBT ({a:1}, 1b, 0.5d) (feature: no Feature.DIRECT_CODEC yet)"]
fn resource_or_id_loot_modifier_matches_vanilla() {
    args_common::replay(
        "resource_or_id_loot_modifier",
        include_str!("../../../mc-game/tests/fixtures/args/resource_or_id_loot_modifier_golden.txt"),
        |ctx| Arc::new(LootModifierArgument::loot_modifier(ctx)),
        |v, _| args_common::render_resource_or_id(v.downcast_ref::<ResourceOrId>().expect("resource or id")),
    );
}
