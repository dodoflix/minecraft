//! Crop, stem, sapling, mushroom and bone meal goldens: scenarios `tests/oracle/scenarios/blk_*.txt` replayed on the real vanilla
//! level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_*.txt`).

use crate::common;

macro_rules! golden {
    ($($name:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $name() {
            common::block_scenario::check($scenario);
        }
    )+};
}

golden!(
    crops_growth => "blk_crops_growth",
    crops_misc_growth => "blk_crops_misc_growth",
    stems => "blk_stems",
    plants_survival => "blk_plants_survival",
    bonemeal_simple => "blk_bonemeal_simple",
    bonemeal_water => "blk_bonemeal_water",
    saplings_single => "blk_saplings_single",
    saplings_mega => "blk_saplings_mega",
    saplings_flowers => "blk_saplings_flowers",
    saplings_random_ticks => "blk_saplings_random_ticks",
    mushroom_spread => "blk_mushroom_spread",
    mushroom_bonemeal => "blk_mushroom_bonemeal",
    nether_fungus => "blk_nether_fungus",
    eyeblossom => "blk_eyeblossom",
);
