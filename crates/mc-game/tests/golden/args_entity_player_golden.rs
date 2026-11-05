//! `EntityArgument::player()` (selectors, names, UUIDs) against the vanilla golden (`fixtures/args/entity_player_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::selector::EntitySelector;
use mc_game::commands::arguments::EntityArgument;
use std::sync::Arc;

#[test]
fn entity_player_matches_vanilla() {
    args_common::replay(
        "entity_player",
        include_str!("../fixtures/args/entity_player_golden.txt"),
        |_| Arc::new(EntityArgument::player()),
        |v, _| v.downcast_ref::<EntitySelector>().expect("selector").describe(),
    );
}
