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
