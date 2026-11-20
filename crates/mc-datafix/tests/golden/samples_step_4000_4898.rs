//! STEP samples of the fixes of data versions 4054..4892: schema-driven inputs (and hand written ones) recorded with
//! the real vanilla fixer (`fixtures/samples/record.sh step_4000_4898 12`), replayed through ours, and real chunks,
//! entity chunks, POI chunks, saved data and level files of the worlds saved by the 1.20.6 to 26.1 servers
//! (`fixtures/samples/step_real_4000_4898.tsv.gz`: the richest and a random chunk per world and dimension, every saved
//! data file, the level file whole and its `Data` compound; each updated from the stored version to 4898).

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_1_21_2_to_1_21_11_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_4000_4898.tsv.gz");
    assert_replays(&fixture, 5675);
}

#[test]
fn real_worlds_of_the_1_20_6_to_26_1_servers_update_like_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_real_4000_4898.tsv.gz");
    assert_replays(&fixture, 264);
}
