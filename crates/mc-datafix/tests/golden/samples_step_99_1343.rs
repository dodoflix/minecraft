//! STEP samples of the fixes of data versions 99..1343 recorded with the real vanilla fixer and replayed through ours:
//! schema-driven and hand written inputs of every fix version (`fixtures/samples/record.sh step_99_1343 8`). The real
//! chunks and level files of the 1.7.10 to 1.12.2 servers are part of the corpus fixtures (`samples_corpus`).

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_legacy_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_99_1343.tsv.gz");
    assert_replays(&fixture, 3128);
}
