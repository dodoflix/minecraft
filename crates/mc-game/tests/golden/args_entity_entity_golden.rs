//! `EntityArgument::entity()` (selectors, names, UUIDs) against the vanilla golden (`fixtures/args/entity_entity_golden.txt`).

use crate::args_common;

use mc_game::commands::arguments::selector::EntitySelector;
use mc_game::commands::arguments::EntityArgument;
use std::sync::Arc;

#[test]
fn entity_entity_matches_vanilla() {
    args_common::replay(
        "entity_entity",
        include_str!("../fixtures/args/entity_entity_golden.txt"),
        |_| Arc::new(EntityArgument::entity()),
        |v, _| v.downcast_ref::<EntitySelector>().expect("selector").describe(),
    );
}
