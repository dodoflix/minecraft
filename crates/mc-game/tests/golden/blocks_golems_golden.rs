//! Goldens of `CarvedPumpkinBlock`, `PumpkinBlock`, `WitherSkullBlock`, `HugeMushroomBlock`, `ShelfMushroomBlock` and the
//! `SculkBlock` experience: scenarios `tests/oracle/scenarios/blk_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

use crate::common;

#[test]
fn pumpkin_golems_match_vanilla() {
    common::block_scenario::check("blk_pumpkin_golems");
}

#[test]
fn golem_copper_matches_vanilla() {
    common::block_scenario::check("blk_golem_copper");
}

#[test]
fn wither_pattern_matches_vanilla() {
    common::block_scenario::check("blk_wither_pattern");
}

#[test]
fn mushroom_blocks_place_matches_vanilla() {
    common::block_scenario::check("blk_mushroom_blocks_place");
}

#[test]
fn sculk_break_xp_matches_vanilla() {
    common::block_scenario::check("blk_sculk_break_xp");
}
