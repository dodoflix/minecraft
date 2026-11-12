//! Command block state machine, structure block, end gateway and creaking heart against the real vanilla level: scenarios
//! `tests/oracle/scenarios/blk_{cmdblock_chain,cmdblock_place,structure_block,end_gateway,creaking_heart}.txt`, and the entity-driven
//! block hooks (`blk_landing_blocks`, `blk_entity_wind_charge`, `blk_entity_hooks`).
use crate::common::block_scenario::check;

#[test]
fn command_block_chain_matches_vanilla() {
    check("blk_cmdblock_chain");
}

#[test]
fn command_block_placement_and_editor_match_vanilla() {
    check("blk_cmdblock_place");
}

#[test]
fn structure_block_matches_vanilla() {
    check("blk_structure_block");
}

#[test]
fn end_gateway_matches_vanilla() {
    check("blk_end_gateway");
}

#[test]
fn creaking_heart_without_creaking_matches_vanilla() {
    check("blk_creaking_heart");
}

#[test]
fn slime_honey_and_hay_landing_matches_vanilla() {
    check("blk_landing_blocks");
}

#[test]
fn wind_charge_explosion_hits_blocks_like_vanilla() {
    check("blk_entity_wind_charge");
}

#[test]
fn entity_driven_block_hooks_match_vanilla() {
    check("blk_entity_hooks");
}
