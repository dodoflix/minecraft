//! STEP samples of the fixes of data versions 2200..2699: schema-driven inputs (and a few hand written ones) recorded
//! with the real vanilla fixer (`fixtures/samples/record.sh step_2200_2699 30`), replayed through ours.

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_1_16_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_2200_2699.tsv.gz");
    assert_replays(&fixture, 3791);
}
