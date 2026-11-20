//! STEP samples of the fixes of data versions 2700..3299: schema-driven inputs (and hand written ones) recorded with
//! the real vanilla fixer (`fixtures/samples/record.sh step_2700_3299 30`), replayed through ours.

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_1_17_to_1_19_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_2700_3299.tsv.gz");
    assert_replays(&fixture, 2614);
}
