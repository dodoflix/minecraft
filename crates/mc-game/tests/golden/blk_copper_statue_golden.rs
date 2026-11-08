//! Goldens of `CopperGolemStatueBlock`, `WeatheringCopperGolemStatueBlock` and `CopperGolemStatueBlockEntity`: scenario
//! `tests/oracle/scenarios/blk_copper_statue.txt` replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our
//! ticking level (fixture `tests/fixtures/entity_oracle/blk_copper_statue.txt`).

use crate::common;

#[test]
fn copper_golem_statue_matches_vanilla() {
    common::block_scenario::check("blk_copper_statue");
}
