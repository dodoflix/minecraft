//! Goldens of the sculk family and the vibration system: scenarios `tests/oracle/scenarios/blk_vib_*.txt` replayed on
//! the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures
//! `tests/fixtures/entity_oracle/blk_vib_*.txt`).

use crate::common;

macro_rules! golden {
    ($($name:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            common::block_scenario::check(concat!("blk_vib_", stringify!($name)));
        }
    )+};
}

golden!(
    place_break,
    frequencies,
    selector,
    calibrated,
    shrieker,
    shrieker_sources,
    shrieker_step,
    wool,
    catalyst,
    catalyst_multi,
    be_nbt,
    running_sources
);

/// The arrow's own state (`ground`, `crit`, ...) is covered by the arrow golden and not part of this one, nor is the
/// item entity's `pickup` delay.
#[test]
fn entities() {
    common::block_scenario::check_with(
        "blk_vib_entities",
        &common::entity_oracle::Options {
            ignore_fields: vec!["ground", "crit", "pierce", "pickup", "shake", "life", "dmg"],
            ..Default::default()
        },
    );
}
