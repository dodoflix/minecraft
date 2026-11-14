//! Block scenarios of sculk (catalyst cursors, veins, shrieker) and the hatching blocks (turtle eggs, frogspawn, sniffer
//! eggs, infested blocks) (`tests/oracle/scenarios/blk_{sculk_spread,sculk_shrieker,eggs_hatch}.txt`) replayed on the real
//! vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

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
    sculk_spread => "blk_sculk_spread",
    sculk_shrieker => "blk_sculk_shrieker",
    eggs_hatch => "blk_eggs_hatch",
);
