//! Ender pearls against vanilla: `EnderpearlItem.use`, `ThrownEnderpearl` flight, landing and owner states (scenarios
//! `tests/oracle/scenarios/itm_throw_pearl*.txt`, `ent_pearl_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn throw_pearl() {
    common::block_scenario::check("itm_throw_pearl");
}

#[test]
fn throw_pearl_peaceful() {
    common::block_scenario::check("itm_throw_pearl_peaceful");
}

#[test]
fn pearl_hit_entity() {
    common::block_scenario::check("ent_pearl_hit_entity");
}

#[test]
fn pearl_owner_states() {
    common::block_scenario::check("ent_pearl_owner_states");
}

#[test]
fn pearl_persist() {
    common::block_scenario::check("ent_pearl_persist");
}

/// A pearl that lands in another level than its owner teleports the owner into the pearl's level.
#[test]
fn pearl_cross_dimension() {
    common::block_scenario::check("ent_pearl_cross_dimension");
}

/// A pearl in another level than its owner is saved with that level, removed with the owner, and spawned there again when the owner joins with
/// its saved data.
#[test]
fn pearl_cross_dimension_rejoin() {
    common::block_scenario::check("ent_pearl_cross_dimension_rejoin");
}

/// A pearl whose owner died in another level vanishes in its next tick (the owner is found through the player list), or flies on with the rule off.
#[test]
fn pearl_cross_dimension_dead_owner() {
    common::block_scenario::check("ent_pearl_cross_dimension_death");
}

/// A pearl finds the respawned owner by UUID and teleports the new player when it lands.
#[test]
fn pearl_finds_a_respawned_owner() {
    common::block_scenario::check("ent_pearl_respawn");
}
