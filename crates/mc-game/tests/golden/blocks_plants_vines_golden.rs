//! Growing plant goldens: scenarios `tests/oracle/scenarios/blk_*.txt` (vines, cave/nether vines, bamboo, sugar cane and cactus, chorus,
//! dripleaf, glow lichen and moss, resin clump) replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level
//! (fixtures `tests/fixtures/entity_oracle/blk_*.txt`).

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
    vine_spread => "blk_vine_spread",
    cave_vines => "blk_vines_cave",
    nether_vines => "blk_vines_nether",
    vines_survival => "blk_vines_survival",
    bamboo => "blk_bamboo",
    cane_cactus => "blk_cane_cactus",
    chorus => "blk_chorus",
    dripleaf => "blk_dripleaf",
    lichen_moss => "blk_lichen_moss",
    resin_clump => "blk_plants_resin_clump",
);
