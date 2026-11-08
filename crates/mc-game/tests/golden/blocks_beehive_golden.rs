//! Beehive and bee nest goldens: scenarios `tests/oracle/scenarios/blk_beehive_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_beehive_*.txt`).
//! Hives never hold a released bee in the recordings: no bee entity exists, so occupants that leave the hive are pigs
//! (not `beehive_inhabitors`) and bees only stay in smoked or silk-touched hives.

use crate::common;

#[test]
fn bottle_and_shears_harvest_matches_vanilla() {
    common::block_scenario::check("blk_beehive_harvest");
}

#[test]
fn breaking_exploding_and_placing_matches_vanilla() {
    common::block_scenario::check("blk_beehive_break");
}

#[test]
fn hive_item_keeps_bees_and_honey_level_matches_vanilla() {
    common::block_scenario::check("blk_beehive_carry");
}
