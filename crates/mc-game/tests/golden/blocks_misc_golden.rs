//! Block scenarios of the explosion, door-destruction and bed rules (`tests/oracle/scenarios/blk_fbc_{tnt,dragon_egg,respawn_anchor,beds}.txt`,
//! `blk_infra_explode_drops.txt`, `blk_dc_door_destroy.txt`) replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level
//! (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

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
    tnt => "blk_fbc_tnt",
    dragon_egg => "blk_fbc_dragon_egg",
    respawn_anchor => "blk_fbc_respawn_anchor",
    explode_drops => "blk_infra_explode_drops",
    door_destroy => "blk_dc_door_destroy",
    beds => "blk_fbc_beds",
);
