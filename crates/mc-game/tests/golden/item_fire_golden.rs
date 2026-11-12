//! Flint and steel and fire charge `useOn` (`tests/oracle/scenarios/itm_fire_ignite{,_nether}.txt`) replayed on the real vanilla
//! level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

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
    fire_ignite => "itm_fire_ignite",
    fire_ignite_nether => "itm_fire_ignite_nether",
);
