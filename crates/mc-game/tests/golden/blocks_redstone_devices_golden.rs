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
    daylight,
    lever_button,
    clicks,
    dust_click,
    dust_click_exp,
    target,
    rod,
    analog_sources,
    tripwire
);

/// Item entities on plates: the item's `delta` movement is not part of the comparison (it depends on the entity's random seed).
#[test]
fn plates() {
    common::block_scenario::check_with(
        "blk_rd_plates",
        &common::entity_oracle::Options {
            ignore_fields: vec!["delta"],
            ..Default::default()
        },
    );
}
