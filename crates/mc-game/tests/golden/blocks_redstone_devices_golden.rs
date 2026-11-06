//! Redstone device goldens: scenarios `tests/oracle/scenarios/blk_rd_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_rd_*.txt`).

use crate::common;

macro_rules! golden {
    ($($name:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            common::block_scenario::check(concat!("blk_rd_", stringify!($name)));
        }
    )+};
}

golden!(
    repeater,
    comparator,
    observer,
    copper_bulb,
    noteblock,
    daylight
);
