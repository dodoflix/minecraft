//! Beehive and bee nest goldens: scenarios `tests/oracle/scenarios/blk_beehive_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_beehive_*.txt`).
//! Occupants that leave the hive in these recordings are pigs (not `beehive_inhabitors`, so the release path runs without a bee
//! entity); bees that do leave are replayed by `mob_bee_golden.rs` (`mob_bee_hive_*`).

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
