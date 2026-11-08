//! Plain block-side hooks against the real vanilla level: game master blocks and the op permission, composters as
//! `WorldlyContainerHolder`s, piglins angered by guarded blocks. Scenarios: `tests/oracle/scenarios/blk_light_barrier.txt`,
//! `blk_composter_hopper.txt`, `blk_piglin_gold_break.txt`; fixtures from `scripts/block-oracle.sh NAME`.
use crate::common::block_scenario::check;

#[test]
fn light_and_barrier_need_game_master_permission() {
    check("blk_light_barrier");
}

#[test]
fn composter_is_a_worldly_container_for_hoppers_and_droppers() {
    check("blk_composter_hopper");
}

#[test]
fn breaking_gold_angers_idle_piglins() {
    check("blk_piglin_gold_break");
}

#[test]
fn game_master_blocks_refuse_players_without_permission() {
    check("blk_gamemaster_blocks");
}
