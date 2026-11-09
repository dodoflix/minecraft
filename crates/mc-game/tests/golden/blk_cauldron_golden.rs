//! Goldens of the cauldron blocks, candles and cakes, waterlogging: scenarios `tests/oracle/scenarios/NAME.txt` replayed on the
//! real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

use crate::common;

#[test]
fn cauldron_interactions_match_vanilla() {
    common::block_scenario::check("blk_fbc_cauldron_interactions");
}

#[test]
fn dripstone_fills_cauldrons_and_mud_turns_to_clay() {
    common::block_scenario::check("blk_dripstone_transfer");
}

#[test]
fn cauldron_entity_inside_shapes_match_vanilla() {
    common::block_scenario::check("blk_fluid_cauldron_inside");
}

#[test]
fn candles_and_cakes_match_vanilla() {
    common::block_scenario::check("blk_fbc_candles_cake");
}

#[test]
fn waterlogging_through_every_container_matches_vanilla() {
    common::block_scenario::check("blk_fluid_waterlog");
}

#[test]
fn rain_fills_cauldrons_like_vanilla() {
    common::block_scenario::check("blk_fluid_cauldron_rain");
}

#[test]
fn snow_fills_cauldrons_in_a_cold_biome_like_vanilla() {
    common::block_scenario::check("blk_fluid_cauldron_snow");
}
