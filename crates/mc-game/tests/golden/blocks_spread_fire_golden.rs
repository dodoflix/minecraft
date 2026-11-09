//! Block scenarios of the spreading blocks (grass, nylium, paths, snow, ice, leaves), fire and pointed dripstone, sulfur
//! (`tests/oracle/scenarios/blk_{sl_*,fbc_fire_*,dripstone_{fall,place},potent_sulfur}.txt`) replayed on the real vanilla level
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
    grass_spread => "blk_sl_spread",
    grass_bonemeal => "blk_sl_grass_bonemeal",
    leaves => "blk_sl_leaves",
    snow_and_ice => "blk_sl_snow_ice",
    nylium => "blk_sl_nether",
    path_and_concrete_powder => "blk_sl_path_concrete",
    fire_rain => "blk_fbc_fire_rain",
    fire_spread => "blk_fbc_fire_spread",
    fire_ignite => "blk_fbc_fire_ignite",
    dripstone_fall => "blk_dripstone_fall",
    dripstone_place => "blk_dripstone_place",
    potent_sulfur => "blk_potent_sulfur",
);
