//! Entity tracking in the block replay: when `remove_entities` is sent relative to the packets of the same tick, the order of a
//! dropped item's packets, and the entity ticking range (`tests/oracle/scenarios/ent_{remove_order,arrow_discard_order,drop_order,ticking_range}.txt`;
//! fixtures regenerated with `scripts/block-oracle.sh NAME`).

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
    arrow_discard_order => "ent_arrow_discard_order",
    drop_order => "ent_drop_order",
    remove_order => "ent_remove_order",
    ticking_range => "ent_ticking_range",
);
