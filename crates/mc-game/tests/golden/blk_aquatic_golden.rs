//! Block scenarios of the aquatic blocks and of the fluid gamerule/dimension/blocked-flow cases
//! (`tests/oracle/scenarios/blk_{aquatic_*,fluid_{lava_nether,gamerules,blocked_flow}}.txt`) replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

use crate::common;

macro_rules! golden {
    ($($test:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $test() {
            common::block_scenario::check($scenario);
        }
    )+};
}

golden!(
    sponge => "blk_aquatic_sponge",
    sponge_dry => "blk_aquatic_sponge_dry",
    sponge_nether => "blk_aquatic_sponge_nether",
    coral => "blk_aquatic_coral",
    bubble_column => "blk_aquatic_bubble_column",
    plants => "blk_aquatic_plants",
    bonemeal => "blk_aquatic_bonemeal",
    kelp => "blk_aquatic_kelp",
    waterlog_place => "blk_aquatic_waterlog_place",
    lava_nether => "blk_fluid_lava_nether",
    fluid_gamerules => "blk_fluid_gamerules",
    blocked_flow => "blk_fluid_blocked_flow",
);
