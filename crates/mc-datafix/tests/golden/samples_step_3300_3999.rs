//! STEP samples of the fixes of data versions 3300..3999: schema-driven inputs (and hand written ones) recorded with
//! the real vanilla fixer (`fixtures/samples/record.sh step_3300_3999 30`), replayed through ours.

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_1_19_4_to_1_21_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_3300_3999.tsv.gz");
    assert_replays(&fixture, 7714);
}
